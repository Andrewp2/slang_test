// src/shader_compiler.cpp

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

#define RETURN_ON_FAIL(x) \
  {                       \
    auto _res = x;        \
    if (_res != 0)        \
    {                     \
      return -1;          \
    }                     \
  }

namespace fs = std::filesystem;

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

int main(int argc, char *argv[])
{
  (void)argc; // Suppress unused parameter warnings
  (void)argv;

  fs::path out_dir = "compiled_shaders";
  fs::create_directories(out_dir);
  ComPtr<slang::IGlobalSession> slangGlobalSession;
  RETURN_ON_FAIL(slang::createGlobalSession(slangGlobalSession.writeRef()));
  slang::TargetDesc targetDesc = {};
  targetDesc.format = SLANG_GLSL;
  targetDesc.profile = slangGlobalSession->findProfile("glsl_460");
  slang::SessionDesc sessionDesc = {};
  sessionDesc.targets = &targetDesc;
  sessionDesc.targetCount = 1;
  ComPtr<slang::ISession> session;
  RETURN_ON_FAIL(slangGlobalSession->createSession(sessionDesc, session.writeRef()));
  nlohmann::json reflection_data = nlohmann::json::array();
  // for (const auto &entry : fs::recursive_directory_iterator("assets/shaders"))
  // {
  //   if (entry.is_regular_file() && entry.path().extension() == ".slang")
  //   {
  //     try
  //     {
  //       fs::path shader_path = entry.path();
  //       std::string shader_name = shader_path.stem().string();
  //       std::cout << "Compiling shader: " << shader_name << std::endl;

  //       ComPtr<slang::IBlob> diagnosticBlob;
  //       ComPtr<slang::IModule> slangModule;
  //       {
  //         slangModule = session->loadModule(shader_name.c_str(), diagnosticBlob.writeRef());
  //         diagnoseIfNeeded(diagnosticsBlob);
  //         if (!slangModule)
  //         {
  //           return -1;
  //         }
  //       }
  //       ComPtr<slang::IEntryPoint> entryPoint;
  //       SlangResult result = slangModule->findEntryPointByName("main", entryPoint.writeRef());
  //       if (SLANG_FAILED(result) || !entryPoint)
  //       {
  //         std::cerr << "Failed to find entry point 'main' in shader: " << shader_name << std::endl;
  //         continue;
  //       }

  //       slang::IComponentType *componentTypes[] = {slangModule.get(), entryPoint.get()};
  //       ComPtr<slang::IComponentType> composedProgram;
  //       {
  //         ComPtr<slang::IBlob> diagnosticsBlob;
  //         result = session->createCompositeComponentType(
  //             componentTypes, 2, composedProgram.writeRef(), diagnosticsBlob.writeRef());

  //         if (diagnosticsBlob)
  //         {
  //           std::cerr << (const char *)diagnosticsBlob->getBufferPointer() << std::endl;
  //         }
  //         if (SLANG_FAILED(result) || !composedProgram)
  //         {
  //           std::cerr << "Failed to create composite component for shader: " << shader_name << std::endl;
  //           continue;
  //         }
  //       }
  //       int targetIndex = composedProgram->addTarget(targetDesc);
  //       ComPtr<slang::IBlob> glslBlob;
  //       {
  //         ComPtr<slang::IBlob> diagnosticsBlob;
  //         result = composedProgram->getEntryPointCode(
  //             0, targetIndex, glslBlob.writeRef(), diagnosticsBlob.writeRef());
  //         if (diagnosticsBlob)
  //         {
  //           std::cerr << (const char *)diagnosticsBlob->getBufferPointer() << std::endl;
  //         }
  //         if (SLANG_FAILED(result) || !glslBlob)
  //         {
  //           std::cerr << "Failed to get compiled code for shader: " << shader_name << std::endl;
  //           continue;
  //         }
  //       }

  //       fs::path output_shader_path = out_dir / (shader_name + ".glsl");
  //       std::ofstream shader_file(output_shader_path);
  //       shader_file.write(
  //           (const char *)glslBlob->getBufferPointer(), glslBlob->getBufferSize());
  //       shader_file.close();

  //       nlohmann::json shader_reflection;
  //       shader_reflection["shader_name"] = shader_name;

  //       slang::ProgramLayout *program_layout = composedProgram->getLayout();
  //       if (program_layout)
  //       {
  //         unsigned parameterCount = program_layout->getParameterCount();
  //         nlohmann::json parameters = nlohmann::json::array();
  //         for (unsigned pp = 0; pp < parameterCount; pp++)
  //         {
  //           slang::VariableLayoutReflection *parameter = program_layout->getParameterByIndex(pp);
  //           const char *parameterName = parameter->getName();
  //           nlohmann::json param_info;
  //           param_info["name"] = parameterName ? parameterName : "";
  //           slang::TypeLayoutReflection *typeLayout = parameter->getTypeLayout();
  //           if (typeLayout)
  //           {
  //             param_info["type"] = typeLayout->getType()->getName();
  //           }
  //           parameters.push_back(param_info);
  //         }
  //         shader_reflection["parameters"] = parameters;

  //         unsigned entryPointCount = program_layout->getEntryPointCount();
  //         nlohmann::json entry_points = nlohmann::json::array();
  //         for (unsigned ee = 0; ee < entryPointCount; ee++)
  //         {
  //           slang::EntryPointLayout *entryPointLayout = program_layout->getEntryPointByIndex(ee);
  //           if (entryPointLayout)
  //           {
  //             nlohmann::json entry_info;
  //             entry_info["name"] = entryPointLayout->getName();
  //             entry_info["stage"] = entryPointLayout->getStage();
  //             entry_points.push_back(entry_info);
  //           }
  //         }
  //         shader_reflection["entry_points"] = entry_points;
  //       }

  //       reflection_data.push_back(shader_reflection);
  //     }
  //     catch (const std::exception &e)
  //     {
  //       std::cerr << "Error processing shader " << entry.path() << ": " << e.what() << std::endl;
  //       continue;
  //     }
  //   }
  // }

  fs::path reflection_path = out_dir / "reflection.json";
  std::ofstream reflection_file(reflection_path);
  reflection_file << reflection_data.dump(4);
  reflection_file.close();

  return 0;
}
