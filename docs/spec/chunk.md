# Chunk de tiles (`.bmc`) e manifesto de nível (`level.toml`) — spec v0

## Manifesto de nível

Um **nível** é uma região (país, estado ou município) renderizada como uma grade de células.
Fica em um diretório com `level.toml` e os chunks:

```
<nivel>/<entity-id>/
  level.toml
  chunks/<cx>_<cy>.bmc
```

Exemplo: `3/municipio:4314902/level.toml` (Porto Alegre). Em sistemas de arquivos que não
aceitam `:` no nome, usar `_` (`municipio_4314902`); o `level.toml` é a fonte de verdade.

```toml
spec_version = 0

[level]
level = 3                      # 1, 2 ou 3
entity = "municipio:4314902"   # entidade que esta grade representa
name = "Porto Alegre"

[grid]
width = 1024                   # células
height = 1024
chunk_size = 64                # células por lado de chunk (fixo em 64 na v0)
cell_meters = 30.0             # tamanho da célula no terreno

[projection]
crs = "EPSG:5880"              # projeção usada para amostrar a grade
origin_x = 0.0                 # coordenada projetada da célula (0,0)
origin_y = 0.0
rotation_deg = 45.0            # rotação dos eixos da grade em relação ao norte

[tileset]
id = "bramapa-base"            # tileset que os ids de tile referenciam
version = "0.0.1"

[period]                       # opcional; ausente = mapa sem data
start = "2023"
end = "2023"

# Entidades referenciadas pela camada `entity` dos chunks. `index` 0 é reservado (nenhuma).
# A ordem/numeração é livre, mas estável dentro do nível.
[[entities]]
index = 1
id = "estado:43"
name = "Rio Grande do Sul"

[[entities]]
index = 2
id = "pais:UY"
name = "Uruguai"
```

Chunks fora de `[0, width/chunk_size) × [0, height/chunk_size)` são inválidos. Chunks ausentes
são tratados como vazios (oceano/transparente, a critério do motor).

## Arquivo de chunk

Um arquivo por chunk. Cabeçalho não comprimido, carga comprimida com zstd.

### Cabeçalho (32 bytes)

| Offset | Tipo | Campo | Valor |
|---|---|---|---|
| 0 | `[u8; 4]` | magic | `"BMCK"` |
| 4 | `u16` | spec_version | `0` |
| 6 | `u16` | flags | bit 0: tem camada `objects`; bit 1: tem camada `entity` |
| 8 | `u16` | chunk_size | `64` |
| 10 | `u8` | layer_count | `2` + (1 se `entity`) + (1 se `objects`) |
| 11 | `u8` | reserved | `0` |
| 12 | `i32` | cx | coordenada do chunk na grade |
| 16 | `i32` | cy | |
| 20 | `i32` | period_start | dias desde 1970-01-01; `i32::MIN` = sem período |
| 24 | `i32` | period_end | idem |
| 28 | `u32` | payload_len | tamanho da carga **comprimida** em bytes |

### Carga (após descompressão)

Camadas em ordem fixa:

1. **`terrain`**: `chunk_size²` valores `u16`, linha por linha (`y` externo, `x` interno).
   Id de tile no tileset. `0` = vazio.
2. **`overlay`**: `chunk_size²` valores `u16`. Desenhado sobre o terreno (estradas, rios, bordas).
   `0` = vazio.
3. **`entity`** (só se `flags & 2`): `chunk_size²` valores `u16`, índice em `[[entities]]` do
   `level.toml`. `0` = nenhuma entidade (oceano, fora do escopo). É o que permite hover/clique:
   o motor nunca faz teste de ponto em polígono, só lê este índice.
4. **`objects`** (só se `flags & 1`): `u32 count`, depois `count` registros de 8 bytes:

   | Tipo | Campo |
   |---|---|
   | `u8` | cell_x (0..chunk_size) |
   | `u8` | cell_y |
   | `u16` | tile_id (sprite no tileset, classe `object`) |
   | `u16` | flags (bit 0: espelhado horizontalmente; demais reservados) |
   | `u16` | reserved |

   Objetos são desenhados em ordem de profundidade `(cell_x + cell_y)`, depois ordem de
   aparição. Objetos mais altos que uma célula ancoram no centro inferior do losango da célula.

## Regras de renderização que o produtor pode assumir

- O motor desenha terreno, depois overlay, depois objetos, chunk a chunk, com profundidade
  isométrica correta entre chunks vizinhos.
- Animação é propriedade do tile (ver `tileset.md`); o chunk não guarda frame.
- Overrides manuais (ver `addon.md`) substituem chunks inteiros. O motor escolhe, por chunk,
  o de maior `priority`; empate = o primeiro addon carregado.

## Esclarecimentos v0 (decididos na Fase 0)

- **Grade não múltipla de 64**: o número de chunks é `ceil(width / 64) × ceil(height / 64)`.
  Células além de `width`/`height` no último chunk são escritas como `0` e ignoradas pelo motor.
- **Período em dois lugares**: `level.toml [period]` é a fonte de verdade para o nível; o cabeçalho
  de chunk recebe uma cópia (em dias) e existe para chunks fatiados no tempo no futuro. Ou os dois
  campos do cabeçalho são válidos, ou os dois são `i32::MIN`; "só início" é inválido. Divergência
  entre chunk e nível é aviso, não erro.
- **`layer_count` × `flags`**: `layer_count` é redundante com os bits de `flags` e um leitor
  rejeita inconsistência. Camadas ausentes não são escritas: um chunk sem objetos e sem entidade
  tem `flags = 0`, `layer_count = 2` (codificação canônica; codificadores devem produzir bytes
  idênticos para o mesmo chunk). Ordem fixa das camadas na carga: `terrain`, `overlay`,
  `entity` (se houver), `objects` (se houver).
- **Teto de `chunk_size`**: como `cell_x`/`cell_y` dos objetos são `u8`, `chunk_size` nunca
  passará de 256. Na v0 é sempre 64.
- **Tamanho do arquivo**: exatamente `32 + payload_len` bytes; bytes extras são erro.
- **Campos `reserved`** (cabeçalho e objeto) e **bits altos de `flags` de objeto**: escritores
  gravam `0`, leitores ignoram e preservam.
- **Ordem dos objetos no arquivo**: livre. O motor ordena por profundidade (`cell_x + cell_y`)
  e, em empate, pela ordem no arquivo. Mais de um objeto na mesma célula é permitido.
- **zstd**: o frame deve carregar o tamanho descomprimido (content size), para o leitor
  pré-alocar.
- **`cell_meters`** é sempre float em TOML (`8000.0`, não `8000`).
- **Eixos da grade**: com `rotation_deg = 45`, o eixo `x` da grade aponta para sudeste e o `y`
  para sudoeste no terreno, de modo que o norte geográfico fica para cima na tela. Em outras
  palavras, `rotation_deg` é o ângulo horário entre o norte e o eixo `−y` da grade.
- **Layout de diretório**: um leitor aceita qualquer diretório que contenha `level.toml` e
  `chunks/`; o caminho `<nivel>/<entity-id>/` é convenção de publicação, não exigência.
