# Dados binários (`.bmd`) — spec v0

O build do `bramapa-data` transforma cada propriedade de cada addon em **um arquivo colunar
comprimido**, pronto para o motor carregar sem parsear CSV. Implementado na Fase 3 nos dois
lados (builder Python em `bramapa-data`, leitor Rust em `bramapa-core::data`).

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

## Esclarecimentos v0 (decididos na Fase 3)

- **Tabela de entidades**: ordenada lexicograficamente (ordem de bytes do UTF-8) e sem
  repetição; o índice local é a posição nessa ordem. O leitor pode fazer busca binária.
- **Tabela de strings** (dtype `str`): mesma regra, ordenada e deduplicada.
- **Ordem das linhas**: `(entity_idx, period_start, period_end)`; o desempate por `period_end`
  não afeta leitores que buscam por `(entity_idx, period_start)`.
- **`period_min`/`period_max` no `index.toml`**: datas ISO completas `YYYY-MM-DD`
  (`period_min` = menor `period_start`, `period_max` = maior `period_end`). O builder também
  escreve `period_min_days`/`period_max_days` (i32) e omite as quatro chaves quando a propriedade
  não tem linhas (`rows = 0` continua presente).
- **Chaves extras no `index.toml`**: `generator` no topo e, por propriedade, `addon`,
  `entity_type`, `name`, `unit`, `dtype`, `periodicity`. Leitores ignoram chaves desconhecidas e
  usam a cópia do manifesto como fonte dos metadados.
- **zstd**: um único frame, nível 3, com content size, sem checksum, sem dicionário.
- **Propriedade vazia**: arquivo válido com `row_count = 0`, `entity_count = 0` e um frame zstd
  de carga vazia.
- **Tipos de entidade de dependência** (`outro-addon/usina`): o diretório de saída usa só o nome
  local (`<addon>/usina/<prop>.bmd`); `index.toml` mantém o nome prefixado.
- **Tamanho do arquivo**: exatamente `24 + payload_len` bytes; bytes extras são erro.
