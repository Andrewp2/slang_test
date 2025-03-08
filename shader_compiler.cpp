#include <slang.h>
#include <slang-com-ptr.h>
#include <iostream>
#include <filesystem>
#include <fstream>
#include <cstdlib>
#include <vector>
#include <string>
#include <nlohmann/json.hpp>

using Slang::ComPtr;
namespace fs = std::filesystem;

#define RETURN_ON_FAIL(x) \
  {                       \
    auto _res = x;        \
    if (_res < 0)         \
    {                     \
      return -1;          \
    }                     \
  }

//-----------------------------------------------------------
// Error Reporting Helpers
//-----------------------------------------------------------
template <typename... TArgs>
inline void reportError(const char *format, TArgs... args)
{
  printf(format, args...);
#ifdef _WIN32
  char buffer[4096];
  sprintf_s(buffer, format, args...);
  _Win32OutputDebugString(buffer);
#endif
}

inline void diagnoseIfNeeded(slang::IBlob *diagnosticsBlob)
{
  if (diagnosticsBlob != nullptr)
  {
    reportError("%s", (const char *)diagnosticsBlob->getBufferPointer());
  }
}

//-----------------------------------------------------------
// Reflection Helper Functions
//-----------------------------------------------------------

// Reflect a single parameter and return its JSON representation.
nlohmann::json reflectParameter(slang::VariableLayoutReflection *param)
{
  nlohmann::json j;
  j["name"] = param->getName() ? param->getName() : "";

  slang::TypeLayoutReflection *typeLayout = param->getTypeLayout();
  if (typeLayout)
  {
    auto typeReflection = typeLayout->getType();
    j["type"] = typeReflection->getName();

    if (typeReflection->getKind() == slang::TypeReflection::Kind::Resource)
    {
      nlohmann::json resource;
      resource["shape"] = std::to_string(typeReflection->getResourceShape());
      resource["access"] = std::to_string(typeReflection->getResourceAccess());
      if (auto resType = typeReflection->getResourceResultType())
      {
        resource["result_type"] = resType->getName();
      }
      resource["binding"] = param->getOffset(slang::ParameterCategory::DescriptorTableSlot);
      resource["space"] = param->getBindingSpace(slang::ParameterCategory::DescriptorTableSlot);
      j["resource"] = resource;
    }
  }
  return j;
}

// Reflect parameters returned by getParameterCount() and from the global parameter block.
nlohmann::json reflectParametersFromProgram(slang::ProgramLayout *program_layout, slang::IMetadata *metadata)
{
  nlohmann::json params = nlohmann::json::array();

  // Reflect parameters from getParameterCount()
  unsigned parameterCount = program_layout->getParameterCount();
  for (unsigned i = 0; i < parameterCount; i++)
  {
    slang::VariableLayoutReflection *param = program_layout->getParameterByIndex(i);

    // Check usage via metadata if available
    bool used = true;
    if (metadata)
    {
      bool isUsed = false;
      SlangResult res = metadata->isParameterLocationUsed((SlangParameterCategory)0, i, 0, isUsed);
      if (SLANG_FAILED(res))
      {
        isUsed = true;
      }
      used = isUsed;
    }
    if (!used)
      continue;

    params.push_back(reflectParameter(param));
  }

  // Reflect parameters hidden in the global container
  slang::VariableLayoutReflection *globalParams = program_layout->getGlobalParamsVarLayout();
  if (globalParams)
  {
    slang::TypeLayoutReflection *globalTypeLayout = globalParams->getTypeLayout();
    if (globalTypeLayout && globalTypeLayout->getKind() == slang::TypeReflection::Kind::Struct)
    {
      unsigned fieldCount = globalTypeLayout->getFieldCount();
      for (unsigned f = 0; f < fieldCount; f++)
      {
        slang::VariableLayoutReflection *field = globalTypeLayout->getFieldByIndex(f);
        params.push_back(reflectParameter(field));
      }
    }
  }
  return params;
}

// Reflect entry point information.
nlohmann::json reflectEntryPoints(slang::ProgramLayout *program_layout)
{
  nlohmann::json entryPoints = nlohmann::json::array();
  unsigned entryPointCount = program_layout->getEntryPointCount();
  for (unsigned i = 0; i < entryPointCount; i++)
  {
    slang::EntryPointLayout *ep = program_layout->getEntryPointByIndex(i);
    if (ep)
    {
      nlohmann::json j;
      j["name"] = ep->getName();
      j["stage"] = ep->getStage();
      entryPoints.push_back(j);
    }
  }
  return entryPoints;
}

// Combine all reflection information into one JSON object.
void reflectShaderProgram(nlohmann::json &shader_reflection, slang::ProgramLayout *program_layout, slang::IMetadata *metadata)
{
  shader_reflection["parameters"] = reflectParametersFromProgram(program_layout, metadata);
  shader_reflection["entry_points"] = reflectEntryPoints(program_layout);
}

//-----------------------------------------------------------
// Main Shader Compiler Logic
//-----------------------------------------------------------
int main(int argc, char *argv[])
{
  (void)argc; // Suppress unused parameter warnings
  (void)argv;

  fs::path out_dir = "assets/compiled_shaders";
  fs::create_directories(out_dir);

  // Create and initialize the global Slang session.
  ComPtr<slang::IGlobalSession> slangGlobalSession;
  RETURN_ON_FAIL(slang::createGlobalSession(slangGlobalSession.writeRef()));

  // Setup target description for GLSL.
  slang::TargetDesc targetDesc = {};
  targetDesc.format = SLANG_GLSL;
  targetDesc.profile = slangGlobalSession->findProfile("glsl_460");

  // Setup session description with search paths.
  slang::SessionDesc sessionDesc = {};
  sessionDesc.targets = &targetDesc;
  sessionDesc.targetCount = 1;
  const char *searchPaths[] = {"assets/shaders"};
  sessionDesc.searchPaths = searchPaths;
  sessionDesc.searchPathCount = 1;

  ComPtr<slang::ISession> session;
  RETURN_ON_FAIL(slangGlobalSession->createSession(sessionDesc, session.writeRef()));

  // Load modules from shaders that have at least one entry point.
  std::vector<ComPtr<slang::IModule>> modules;
  for (const auto &entry : fs::recursive_directory_iterator("assets/shaders"))
  {
    if (entry.is_regular_file() && entry.path().extension() == ".slang")
    {
      std::string shader_name = entry.path().stem().string();
      std::cout << "Loading module: " << shader_name << std::endl;

      ComPtr<slang::IBlob> diagnosticBlob;
      ComPtr<slang::IModule> module;
      module.attach(session->loadModule(shader_name.c_str(), diagnosticBlob.writeRef()));

      diagnoseIfNeeded(diagnosticBlob);
      if (!module)
      {
        return -1;
      }
      if (module->getDefinedEntryPointCount() > 0)
      {
        modules.push_back(module);
      }
    }
  }

  nlohmann::json reflection_data = nlohmann::json::array();

  // For each module, compile each entry point separately.
  for (auto &slangModule : modules)
  {
    std::string shader_name = slangModule->getName();
    std::cout << "Compiling shader: " << shader_name << std::endl;

    int entry_point_count = slangModule->getDefinedEntryPointCount();
    std::cout << entry_point_count << " entry points found" << std::endl;

    for (int i = 0; i < entry_point_count; i++)
    {
      std::cout << "Processing entry point " << i << std::endl;
      ComPtr<slang::IEntryPoint> entryPoint;
      SlangResult result = slangModule->getDefinedEntryPoint(i, entryPoint.writeRef());
      std::cout << "Result: " << result << std::endl;
      RETURN_ON_FAIL(result);

      // Create composite component type for the entry point.
      slang::IComponentType *components[] = {entryPoint};
      ComPtr<slang::IComponentType> composedProgram;
      {
        ComPtr<slang::IBlob> diagnosticsBlob;
        result = session->createCompositeComponentType(components, 1, composedProgram.writeRef(), diagnosticsBlob.writeRef());
        std::cout << "Creating composite component type... Result: " << result << std::endl;
        diagnoseIfNeeded(diagnosticsBlob);
        RETURN_ON_FAIL(result);
      }

      // Link the composed program.
      ComPtr<slang::IComponentType> linkedProgram;
      {
        ComPtr<slang::IBlob> diagnosticsBlob;
        result = composedProgram->link(linkedProgram.writeRef(), diagnosticsBlob.writeRef());
        std::cout << "Linking program... Result: " << result << std::endl;
        diagnoseIfNeeded(diagnosticsBlob);
        RETURN_ON_FAIL(result);
      }

      // Generate GLSL blob for the entry point.
      ComPtr<slang::IBlob> glslBlob;
      {
        ComPtr<slang::IBlob> diagnosticsBlob;
        result = linkedProgram->getEntryPointCode(0, 0, glslBlob.writeRef(), diagnosticsBlob.writeRef());
        std::cout << "Generating GLSL blob for entry point " << i << "... Result: " << result << std::endl;
        if (SLANG_FAILED(result))
        {
          if (diagnosticsBlob)
          {
            std::cout << "Diagnostics for entry point " << i << ": "
                      << (const char *)diagnosticsBlob->getBufferPointer()
                      << std::endl;
          }
          RETURN_ON_FAIL(result);
        }
      }

      // Write the GLSL blob to a file.
      std::string outputFileName = shader_name + "_" + std::to_string(i) + ".comp";
      fs::path output_shader_path = out_dir / outputFileName;
      std::ofstream shader_file(output_shader_path);
      shader_file.write((const char *)glslBlob->getBufferPointer(), glslBlob->getBufferSize());
      shader_file.close();
      std::cout << "Wrote GLSL blob to " << outputFileName << std::endl;

      // Reflect the shader program.
      nlohmann::json shader_reflection_json;
      shader_reflection_json["shader_name"] = shader_name;
      slang::ProgramLayout *program_layout = linkedProgram->getLayout();
      std::cout << "Obtained program layout..." << std::endl;
      if (program_layout)
      {
        // Retrieve metadata for the entry point.
        slang::IMetadata *metadata = nullptr;
        ComPtr<slang::IBlob> metadataDiagnostics;
        result = linkedProgram->getEntryPointMetadata(0, 0, &metadata, metadataDiagnostics.writeRef());
        RETURN_ON_FAIL(result);
        reflectShaderProgram(shader_reflection_json, program_layout, metadata);
      }
      reflection_data.push_back(shader_reflection_json);
    }
  }

  // Write the reflection JSON to file.
  fs::path reflection_path = out_dir / "reflection.json";
  std::ofstream reflection_file(reflection_path);
  reflection_file << reflection_data.dump(4);
  reflection_file.close();

  return 0;
}
