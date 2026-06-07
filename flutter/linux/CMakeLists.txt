cmake_minimum_required(VERSION 3.14)
project(spadebox_flutter VERSION 0.1.0 LANGUAGES C)

# Resolve symlinks so relative paths work even when accessed through a symlink.
get_filename_component(CRATE_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/.." REALPATH)
get_filename_component(WORKSPACE_ROOT "${CRATE_ROOT}/.." REALPATH)
set(CARGO_TARGET_DIR "${WORKSPACE_ROOT}/target")

if(CMAKE_BUILD_TYPE STREQUAL "Debug")
  set(CARGO_PROFILE "debug")
  set(CARGO_PROFILE_ARG "")
else()
  set(CARGO_PROFILE "release")
  set(CARGO_PROFILE_ARG "--release")
endif()

set(SO_PATH "${CARGO_TARGET_DIR}/${CARGO_PROFILE}/libspadebox_flutter.so")

# Build the Rust library as part of the CMake build.
add_custom_command(
  OUTPUT "${SO_PATH}"
  COMMAND cargo build ${CARGO_PROFILE_ARG} -p spadebox-flutter
  WORKING_DIRECTORY "${WORKSPACE_ROOT}"
  COMMENT "Compiling Rust crate spadebox-flutter (${CARGO_PROFILE})"
  VERBATIM
)
add_custom_target(spadebox_flutter_cargo ALL DEPENDS "${SO_PATH}")

add_library(spadebox_flutter SHARED IMPORTED GLOBAL)
add_dependencies(spadebox_flutter spadebox_flutter_cargo)
set_target_properties(spadebox_flutter PROPERTIES
  IMPORTED_LOCATION "${SO_PATH}"
)

set(spadebox_flutter_bundled_libraries
  "${SO_PATH}"
  PARENT_SCOPE
)
