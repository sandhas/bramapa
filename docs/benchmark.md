# Benchmark

Gerado por `scripts/bench.sh` em 2026-10-10T17:29Z · host: Linux 7.1.4-arch1-1 x86_64 · bramapa eed90e8-dirty

Cenário fixo (`--bench`): câmera no centro da grade, para cada zoom 1/32 … 2x, 30 quadros de
aquecimento e 120 medidos, renderizando **fora da tela** em 1920×1080 (sem apresentar ao compositor) e esperando
a GPU terminar cada quadro, então o tempo inclui CPU + GPU serializados e não sofre o teto do monitor.
Quadro médio e p99 em milissegundos; fps = 1000 / média. RSS medido no fim do cenário.

### Brasil e vizinhos · `pais:BR` · 1024×1024 células · 256 chunks · 1070490 instâncias

GPU NVIDIA GeForce GTX 1070 (Vulkan, DiscreteGpu) · janela 1920×1080 · perfil release · vsync off · carga+upload 9 ms · RSS 162 MiB

| zoom | quadro médio (ms) | p99 (ms) | fps | chunks | draw calls | instâncias |
|---|---|---|---|---|---|---|
| 1/32x | 1.61 | 2.10 | 622 | 256 | 416 | 1070490 |
| 1/16x | 1.09 | 1.37 | 915 | 158 | 309 | 668320 |
| 1/8x | 0.50 | 1.07 | 2008 | 50 | 131 | 217058 |
| 1/4x | 0.39 | 1.19 | 2541 | 18 | 49 | 79123 |
| 1/2x | 0.32 | 0.57 | 3152 | 8 | 22 | 34871 |
| 1x | 0.29 | 0.56 | 3466 | 4 | 11 | 17323 |
| 2x | 0.27 | 0.50 | 3689 | 4 | 11 | 17323 |

### Rio Grande do Sul · `estado:43` · 1024×1024 células · 256 chunks · 1132506 instâncias

GPU NVIDIA GeForce GTX 1070 (Vulkan, DiscreteGpu) · janela 1920×1080 · perfil release · vsync off · carga+upload 11 ms · RSS 163 MiB

| zoom | quadro médio (ms) | p99 (ms) | fps | chunks | draw calls | instâncias |
|---|---|---|---|---|---|---|
| 1/32x | 1.64 | 1.99 | 610 | 256 | 654 | 1132506 |
| 1/16x | 1.15 | 1.42 | 866 | 158 | 444 | 715379 |
| 1/8x | 0.47 | 0.63 | 2150 | 50 | 150 | 230835 |
| 1/4x | 0.37 | 0.60 | 2738 | 18 | 54 | 82409 |
| 1/2x | 0.31 | 0.52 | 3203 | 8 | 24 | 36747 |
| 1x | 0.28 | 0.46 | 3587 | 4 | 12 | 18611 |
| 2x | 0.30 | 0.58 | 3369 | 4 | 12 | 18611 |

### Porto Alegre · `municipio:4314902` · 1472×1472 células · 529 chunks · 2594353 instâncias

GPU NVIDIA GeForce GTX 1070 (Vulkan, DiscreteGpu) · janela 1920×1080 · perfil release · vsync off · carga+upload 24 ms · RSS 171 MiB

| zoom | quadro médio (ms) | p99 (ms) | fps | chunks | draw calls | instâncias |
|---|---|---|---|---|---|---|
| 1/32x | 3.15 | 3.68 | 317 | 457 | 1230 | 2267551 |
| 1/16x | 1.40 | 1.77 | 715 | 161 | 465 | 877030 |
| 1/8x | 0.56 | 0.85 | 1787 | 49 | 147 | 285749 |
| 1/4x | 0.35 | 0.52 | 2851 | 17 | 51 | 91198 |
| 1/2x | 0.33 | 0.60 | 2991 | 7 | 21 | 37396 |
| 1x | 0.29 | 0.46 | 3507 | 5 | 15 | 25221 |
| 2x | 0.29 | 0.45 | 3456 | 5 | 15 | 25221 |

