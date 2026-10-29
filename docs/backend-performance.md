# Why sd-server on ROCm is faster than KoboldCpp on Vulkan

This doc asks whether a ROCm build of KoboldCpp would match sd-server on the
AMD Radeon AI PRO R9700 (RDNA4, gfx1201), and in particular whether it
supports int8 convrot. The analysis was done on 2026-10-29. It uses earlier
benchmark logs, the sd.cpp source and KoboldCpp 1.123.1's binaries. Nothing
was run on the GPUs.

## Summary

- **KoboldCpp doesn't run int8 convrot on the GPU, on any backend.** The ROCm
  port wouldn't change that.
- **int8 convrot isn't what makes sd-server fast here.** On Vulkan, the same
  sd-server and int8 model is 4.7× slower than on ROCm, and slower than
  KoboldCpp's Q8_0.
- **The gap is the backend.** On Mesa 25.0.7, RADV doesn't give the R9700
  cooperative matrix. Vulkan matmuls therefore skip RDNA4's matrix cores
  (WMMA), which hipBLAS/rocBLAS use on ROCm.

## Measurements

These come from the server benchmark of 2026-10-28
([benchmark-servers.html](benchmark-servers.html), raw data in
`tmp/bench-servers/`). The workload was one 1 MP edit with one 1024² ref,
CFG 2 and 20 steps, on a single R9700.

| Server | Backend | Model | Per image |
|---|---|---|---|
| sd-server | ROCm (HIP), flash attention | int8 convrot | **103 s** (~4.8 s/it) |
| sd-server | Vulkan (RADV), flash attention | int8 convrot | **480 s** |
| KoboldCpp 1.123.1 | Vulkan (RADV) | Q8_0 GGUF | **~204 s** |
| KoboldCpp 1.123.1 | Vulkan (RADV) | int8 convrot | fails (29 GiB, no image) |

The first two rows use the same binary source, the same model files and the
same convrot code path, so the 4.7× gap between them is the backend alone.

## int8 convrot in sd.cpp

sd-server is built against sd.cpp's patched ggml, which adds a
`GGML_OP_QUANTIZE_I8_CONVROT` op and INT8 matmuls
(`/mnt/storage/sdcpp/src/docs/int8_convrot.md`).

- **Format.** Each linear layer stores I8 weights that were Hadamard-rotated
  offline, with one scale per output row. At runtime the activations go
  through the same rotation, are quantized to I8 per row, multiplied I8×I8
  into I32, and then rescaled.
- **HIP (CDNA, RDNA3, RDNA4).** Fused rotation and quantization kernels for
  group size 256, and hipBLAS for the I8 GEMM.
- **Vulkan.** Native kernels only where packed INT8 dot products are
  available; everything else falls back to the CPU through the scheduler.
- **Upstream ggml.** With `SD_USE_UPSTREAM_GGML=ON`, sd.cpp rejects these
  files.

## int8 convrot in KoboldCpp

These findings come from the strings in
`tmp/koboldcpp/unpacked/_internal/koboldcpp_vulkan.so` (1.123.1), not from
its source.

- **No convrot op.** `QUANTIZE_I8_CONVROT` and the
  `ggml_quantize_i8_convrot` kernels are absent, though sd-server's Vulkan
  build has both.
- **Built against upstream ggml.** The library contains sd.cpp's message
  "Using upstream GGML: INT8 tensorwise/convrot is disabled and FP8 weights
  are converted to F16 at load time".
- **Its own loader.** `kcpp_safetensors_quant` reads `comfy_quant`, rebuilds
  the Hadamard signs (`build_convrot_hadamard_signs`) and fills tensor
  storage from the I8 weights (`fill_i8_tensorwise_storage`). The likely
  reading is that it removes the rotation and expands the weights to 16 bits
  at load time. That matches the benchmark: the int8 files grew to 29 GiB of
  VRAM, about twice their size, and the image failed.
- **All builds alike.** KoboldCpp's Vulkan, CUDA and HIP builds share one
  ggml, so a HIP build would lack the op too.
- **The ROCm fork is too old.** The ROCm fork
  ([YellowRoseCx/koboldcpp-rocm](https://github.com/YellowRoseCx/koboldcpp-rocm))
  last released v1.104 on 2025-12-30, for Windows only. That is older than
  1.118, the first KoboldCpp that loads int8 convrot files at all.

## Why Vulkan is slow on the R9700

ggml's Vulkan backend reports each R9700 as:

```text
AMD Radeon AI PRO R9700 (RADV GFX1201) | fp16: 1 | bf16: 0 | int dot: 1 | matrix cores: none
```

`vulkaninfo` (Mesa 25.0.7, Debian 13) lists `VK_KHR_cooperative_matrix` only
for the Intel iGPU, not for the R9700s. Without cooperative matrix,
ggml-vulkan runs matmuls on the ordinary shader cores, using packed int dot
(dp4a) for quantized types. The RDNA4 matrix cores go unused. On ROCm,
hipBLAS/rocBLAS use them for F16 and I8 GEMMs, which is where the 2–4.7×
comes from.

## What would close the gap

Both options below are inferences and still need to be measured.

1. **KoboldCpp built with hipBLAS for gfx1201, using Q8_0 GGUF.** It wouldn't
   have convrot, but its F16 and Q8_0 matmuls would reach the matrix cores
   through rocBLAS and ggml's HIP kernels. Compare it against the ~204 s of
   Vulkan KoboldCpp and the 103 s of sd-server on ROCm.
2. **A newer Mesa with RADV cooperative matrix on RDNA4** (believed to be in
   Mesa 25.1 or 25.2; not checked). Check that `matrix cores` no longer says
   `none`, then rerun KoboldCpp on Vulkan and sd-server on Vulkan.

Run both only when the GPUs are idle; see `scripts/bench-servers.sh`.
