{
  pkgs,
  ...
}:
let
  cutlass = pkgs.fetchgit {
    url = "https://github.com/NVIDIA/cutlass.git";
    rev = "da5e086dab31d63815acafdac9a9c5893b1c69e2"; # v4.4.2
    hash = "sha256-0q9Ad0Z6E/rO2PdM4uQc8H0E0qs9uKc3reHepiHhjEc=";
  };

  flashAttention = pkgs.fetchgit {
    url = "https://github.com/Dao-AILab/flash-attention.git";
    rev = "2c839c33742309ec41e620bf837495ec9926c56e";
    fetchSubmodules = true;
    hash = "sha256-VwEcC3i76/ekhQX/01XAYa5koyQrxhasUd3HurTzJEs=";
  };
in
{
  packages = with pkgs; [
    cudaPackages.cuda_nvcc
    cudaPackages.cuda_cudart
    cudaPackages.libcublas
    cudaPackages.cutlass
    cudaPackages.libcurand
    cudaPackages.cuda_nvrtc
    cudaPackages.cccl
    gcc14
    cargo-nextest
  ];

  languages.rust = {
    enable = true;
    channel = "nightly";
  };

  env = {
    CXX = "${pkgs.gcc14}/bin/g++";
    CUDA_HOME = "${pkgs.cudaPackages.cuda_cudart}";
    CUDA_PATH = "${pkgs.cudaPackages.cuda_nvcc}";
    MIRMIR_CUDA_INCLUDE_PATH = pkgs.lib.makeSearchPath "include" [
      pkgs.cudaPackages.cuda_cudart
      pkgs.cudaPackages.cuda_nvcc
      pkgs.cudaPackages.cccl
    ];
    MIRCUDA_CUTLASS_DIR = "${cutlass}";
    MIRCUDA_FLASH_ATTN_DIR = "${flashAttention}";
    LD_LIBRARY_PATH = "/run/opengl-driver/lib";
  };

  scripts = {
    tui.exec = "cargo run";
    serve.exec = "cargo run -- serve";
    status.exec = "cargo run -- status";
  };

  enterTest = ''
    cargo nextest run --locked --no-fail-fast
  '';
}
