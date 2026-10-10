# Dados binários (`.bmd`) — spec v0 (rascunho)

O build do `bramapa-data` transforma cada propriedade de cada addon em **um arquivo colunar
comprimido**, pronto para o motor carregar sem parsear CSV. Esta spec é rascunho: o builder é
entregue na Fase 3, mas o formato fica definido agora para o motor já reservar a leitura.

```
build/<addon-id>/<entity_type>/<property-id>.bmd
build/<addon-id>/manifest.toml           # cópia do manifesto, para o motor
build/index.toml                         # lista de addons e propriedades disponíveis
```

## Cabeçalho (24 bytes)

| Offset | Tipo | Campo |
|---|---|---|
| 0 | `[u8; 4]` | magic `"BMDT"` |
| 4 | `u16` | spec_version = 0 |
| 6 | `u8` | dtype: 0 = i64, 1 = f64, 2 = str |
| 7 | `u8` | reserved |
| 8 | `u32` | row_count |
| 12 | `u32` | entity_count (tamanho da tabela de strings de entidades) |
| 16 | `u32` | string_count (tabela de valores `str`; 0 se dtype ≠ str) |
| 20 | `u32` | payload_len (comprimido) |

## Carga (zstd)

Colunas em sequência, cada uma com `row_count` elementos:

1. **Tabela de entidades**: `entity_count` strings (`u32 len` + UTF-8). O índice é o id local.
2. **Tabela de strings** (se dtype = str): `string_count` strings.
3. `entity_idx: u32[row_count]`
4. `period_start: i32[row_count]` (dias desde 1970-01-01)
5. `period_end: i32[row_count]`
6. `value`: `i64[row_count]`, `f64[row_count]` ou `u32[row_count]` (índice na tabela de strings)

Linhas ordenadas por `(entity_idx, period_start)`, o que permite busca binária por entidade e
intervalo temporal sem índice adicional.

## `index.toml`

```toml
spec_version = 0

[[addons]]
id = "ibge-core"
version = "0.1.0"
manifest = "ibge-core/manifest.toml"

[[properties]]
id = "ibge-core/municipio/populacao"
file = "ibge-core/municipio/populacao.bmd"
rows = 5570
period_min = "2000"
period_max = "2024"
```
