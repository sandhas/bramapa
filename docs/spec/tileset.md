# Tileset — spec v0

Um tileset é um diretório com:

```
tiles.toml
atlas.png          # RGBA, filtragem nearest, sem premultiply
```

## Geometria

- Losango base de célula: **64 × 32 px** (`tile_width × tile_height`).
- O atlas é uma grade de **cells** de `cell_width × cell_height` px (v0: 64 × 64). Cada tile
  ocupa uma cell; tiles animados ocupam `frames` cells consecutivas na mesma linha.
- A âncora de todo tile é o **centro inferior** da cell. Para tiles de terreno, o losango fica
  encostado na base da cell; tiles altos (árvores, prédios) crescem para cima.
- Escala de exibição é sempre inteira (1x, 2x, 3x...). Nunca filtrar linearmente.

## `tiles.toml`

```toml
spec_version = 0

[tileset]
id = "bramapa-base"
version = "0.0.1"
atlas = "atlas.png"
tile_width = 64
tile_height = 32
cell_width = 64
cell_height = 64
license = "CC-BY-4.0"

# Um registro por tile. `id` é o u16 usado nos chunks; 0 é reservado (vazio).
[[tiles]]
id = 1
name = "grama"
class = "terrain"            # terrain | overlay | object
col = 0                      # posição da cell no atlas
row = 0
frames = 1                   # cells consecutivas na linha
fps = 0                      # 0 = estático
cover = "grama"              # classe de cobertura que este tile representa (terreno/overlay)

[[tiles]]
id = 2
name = "agua"
class = "terrain"
col = 1
row = 0
frames = 4
fps = 4
cover = "agua"

# Autotiling: mapeia (cobertura, máscara de vizinhança) → tile.
# A máscara usa 4 bits (N=1, E=2, S=4, W=8) indicando vizinhos com a MESMA cobertura.
# Se não houver regra para uma máscara, usa-se `default`.
[[autotile]]
cover = "agua"
default = 2
[autotile.masks]
"0"  = 2
"15" = 2
```

## Classes de cobertura (`cover`) da v0

O pipeline classifica cada célula em uma cobertura e o autotiling escolhe o tile. Lista inicial,
alinhada às classes do MapBiomas agregadas:

`oceano`, `agua`, `floresta`, `savana` (cerrado), `caatinga`, `campo` (pampa), `pantanal`,
`agricultura`, `pastagem`, `urbano`, `montanha`, `areia`, `rocha`, `gelo`, `vazio`.

Overlays: `rodovia`, `ferrovia`, `rio`, `limite_estado`, `limite_municipio`, `limite_pais`.

Um tileset pode declarar coberturas extras; o pipeline só as usa se souber classificá-las.

## Animação

`frames > 1` indica que as cells `col..col+frames` na mesma `row` são os frames. O motor calcula
o frame por `floor(tempo * fps) % frames` no shader; não há estado por tile na CPU.

## Ids de tile do placeholder v0

Enquanto não há arte final, os quatro repositórios usam esta tabela como convenção comum. Um
tileset real pode usar outros ids; o que vale é o `tiles.toml` referenciado pelo `level.toml`.

| Classe | Ids |
|---|---|
| terrain | 1 campo · 2 agua (4 frames) · 3 oceano (4 frames) · 4 floresta · 5 areia · 6 urbano · 7 montanha · 8 savana · 9 agricultura · 10 pastagem · 11 caatinga · 12 pantanal · 13 rocha · 14 gelo |
| overlay | 20 rodovia · 21 rio (4 frames) · 22 limite_estado · 23 limite_municipio · 24 limite_pais · 25 ferrovia |
| object | 40 arvore · 41 predio · 42 casa |

`vazio` é o tile `0`. `rocha` e `gelo` têm id reservado mas ainda não são desenhados nem emitidos.

## Esclarecimentos v0 (decididos na Fase 0)

- **Âncora**: a cell de um objeto é sobreposta 1:1 à cell do terreno da mesma célula (mesmo
  centro inferior). O artista desenha a base do objeto ao redor do centro do losango, que fica em
  `(cell_width / 2, cell_height − tile_height / 2)` = `(32, 48)` na v0.
- **Frames**: um tile animado ocupa as cells `col ..= col + frames − 1` (exatamente `frames`
  cells) na mesma `row`.
- **`fps`**: `frames = 1` exige `fps = 0`; `frames > 1` exige `fps > 0`. Qualquer outra combinação
  é erro.
- **`cover`**: obrigatório para `terrain` e `overlay`, proibido para `object`. Vários tiles podem
  compartilhar a mesma cobertura (é isso que o autotiling escolhe). Os alvos de uma regra
  `[[autotile]]` (`default` e `masks`) devem ter a mesma `cover` da regra.
- **Chaves de `masks`**: strings decimais de `"0"` a `"15"`; outras chaves são erro.
- **Atlas**: dimensões múltiplas de `cell_width`/`cell_height`, máximo 4096 × 4096 (limite seguro
  para GPUs móveis), potência de dois recomendada. Duas tiles nunca compartilham uma cell; cells
  não usadas são transparentes.
- **Direção das faixas de overlay** (rodovia, rio, ferrovia): no placeholder, a faixa segue o eixo
  `+x` da grade (entra pelo meio da aresta noroeste e sai pelo meio da sudeste), para que células
  consecutivas em `x` se emendem. Curvas e ramificações virão por autotiling com máscara.
