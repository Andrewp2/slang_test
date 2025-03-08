#!/usr/bin/env python3
import os
import sys
import subprocess
import platform

# Hardcoded paths (adjust as necessary)
SLANG_INCLUDE_PATH = "/home/andrew-peterson/Downloads/slang-2025.6.1-linux-x86_64/include"
SLANG_LIB_PATH = "/home/andrew-peterson/Downloads/slang-2025.6.1-linux-x86_64/lib"

# Source file
SOURCE_FILE = "shader_compiler.cpp"

# Determine executable name based on OS
if platform.system() == "Windows":
    EXE_NAME = "shader_compiler.exe"
else:
    EXE_NAME = "shader_compiler"

# Create an output directory for the executable
BUILD_DIR = os.path.join(os.getcwd(), "sc_out")
os.makedirs(BUILD_DIR, exist_ok=True)
exe_path = os.path.join(BUILD_DIR, EXE_NAME)

# Compile command for shader_compiler.cpp
compile_cmd = [
    "g++",
    SOURCE_FILE,
    "-o", exe_path,
    "-std=c++17",
    "-I", SLANG_INCLUDE_PATH,
    "-I", "/usr/include/nlohmann",
    "-L", SLANG_LIB_PATH,
    "-lslang",
    "-lstdc++",
    "-Wl,-rpath," + SLANG_LIB_PATH,
]

print("Compiling shader compiler...")
try:
    subprocess.run(compile_cmd, check=True)
    print("Compilation succeeded.")
except subprocess.CalledProcessError:
    print("Compilation failed.", file=sys.stderr)
    sys.exit(1)

# Run the compiled shader compiler executable
print("Running shader compiler...")
try:
    process = subprocess.Popen(
        [exe_path],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        universal_newlines=True,
    )
    # Print output line-by-line
    for line in process.stdout:
        print(line.rstrip())
    for line in process.stderr:
        print("Error:", line.rstrip())
    process.wait()
    if process.returncode != 0:
        print("Shader compiler exited with an error.", file=sys.stderr)
        sys.exit(1)
    else:
        print("Shader compiler finished successfully.")
except Exception as e:
    print("Failed to run shader compiler:", e, file=sys.stderr)
    sys.exit(1)

# Now, compile the generated GLSL files to SPIR-V
def compile_to_spirv(glsl_file_path):
    # Define the SPIR-V output file name (same base name, .spv extension)
    spv_file_path = os.path.splitext(glsl_file_path)[0] + ".spv"
    cmd = ["glslangValidator", "-V", glsl_file_path, "-o", spv_file_path]
    try:
        print(f"Compiling {glsl_file_path} to SPIR-V...")
        subprocess.run(cmd, check=True)
        print(f"SPIR-V binary written to {spv_file_path}")
    except subprocess.CalledProcessError as e:
        print(f"Failed to compile {glsl_file_path} to SPIR-V", file=sys.stderr)
        sys.exit(1)

# Directory where your compiled shaders are located (adjust as needed)
compiled_shaders_dir = os.path.join("assets", "compiled_shaders")

# Loop through all GLSL files in the directory and compile them
for file_name in os.listdir(compiled_shaders_dir):
    if file_name.endswith(".comp"):
        glsl_file = os.path.join(compiled_shaders_dir, file_name)
        compile_to_spirv(glsl_file)
