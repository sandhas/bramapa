# Bramapa — especificação dos contratos (spec v0)

Estes documentos definem os formatos que ligam os quatro repositórios do Bramapa:

| Documento | Contrato | Produzido por | Consumido por |
|---|---|---|---|
| [chunk.md](chunk.md) | Chunk de tiles (`.bmc`) e manifesto de nível (`level.toml`) | `bramapa-pipeline`, addons com override manual | `bramapa` (motor) |
| [tileset.md](tileset.md) | Atlas + `tiles.toml` (ids, classes, autotiling, animação) | `bramapa-tileset` | `bramapa-pipeline`, `bramapa` |
| [addon.md](addon.md) | Manifesto de addon (`manifest.toml`) + CSVs de séries temporais | `bramapa-data` e a comunidade | `bramapa-data` (build/validação), `bramapa` |
| [data-binary.md](data-binary.md) | Dados binários colunares (`.bmd`) | build do `bramapa-data` | `bramapa` |

## Versionamento

- `spec_version` é um inteiro. **v0 é instável**: pode quebrar sem aviso enquanto o MVP não existe.
- Todo arquivo ou manifesto declara o `spec_version` que segue. O motor recusa versões que não conhece.
- A partir da v1, mudança incompatível incrementa a versão e o motor mantém leitura da anterior
  por pelo menos uma versão.

## Convenções comuns

- **Codificação binária**: little-endian em tudo. Strings em UTF-8 com prefixo de tamanho `u32`.
- **Compressão**: zstd, frame padrão, sem dicionário (v0).
- **Identificadores de entidade** (`entity id`), string única no formato `<tipo>:<codigo>`:
  - `pais:BR` (ISO 3166-1 alpha-2)
  - `estado:43` (código IBGE da UF, 2 dígitos)
  - `municipio:4314902` (código IBGE do município, 7 dígitos)
  - Tipos declarados por addons: `<tipo>:<codigo-do-addon>`, ex.: `usina:ANEEL-12345`.
- **Identificador canônico de propriedade**: `<addon>/<tipo-de-entidade>/<nome>`,
  ex.: `ibge-core/municipio/populacao`. Dois addons nunca colidem porque o prefixo é o id do addon.
- **Períodos**: datas ISO 8601 (`YYYY`, `YYYY-MM` ou `YYYY-MM-DD`). Um período é `[inicio, fim]`
  inclusivo. `2010` equivale a `2010-01-01` até `2010-12-31`. Em binário, datas são `i32` de dias
  desde 1970-01-01.
- **Níveis**: `1` = país, `2` = estado, `3` = município.
- **Coordenadas de célula**: `(x, y)` inteiros na grade do nível, origem no canto noroeste da
  região, `x` cresce para leste-sul (eixo isométrico direito) e `y` para oeste-sul (eixo esquerdo).
  Como a grade é amostrada com eixos rotacionados 45°, o norte geográfico fica para cima na tela.
