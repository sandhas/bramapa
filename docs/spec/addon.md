# Addon — spec v0

Um addon é um diretório com um `manifest.toml` e arquivos de dados. **Nunca contém código.**
Tudo que um addon pode fazer é declarativo: tipos de entidade, propriedades com série temporal,
regras de exibição e overrides de mapa.

```
addons/<addon-id>/
  manifest.toml
  data/*.csv
  map/<nome>/...           # opcional: chunks de override (ver chunk.md)
  README.md                # opcional
```

## Identificadores

- `addon.id`: `[a-z0-9-]+`, único no ecossistema. É o namespace de tudo que o addon declara.
- Propriedade: `<addon.id>/<entity_type>/<properties[].id>`.
- Tipo de entidade declarado: `<addon.id>/<entity_types[].id>`; na instância, o id de entidade é
  `<entity_types[].id>:<codigo>`.
- Tipos base (sem prefixo, pertencem ao motor): `pais`, `estado`, `municipio`.

## `manifest.toml`

```toml
spec_version = 0

[addon]
id = "ibge-core"
name = "IBGE — dados básicos"
version = "0.1.0"               # semver
license = "CC-BY-4.0"           # licença DOS DADOS deste addon
authors = ["Nome <email>"]
description = "Área, população e PIB por UF e município."
depends = []                    # ids de addons cujos tipos de entidade este usa

# Opcional: tipos de entidade novos.
[[entity_types]]
id = "usina"
name = "Usina de energia"
min_level = 2                   # a partir de qual nível aparece no mapa
position = "latlon"             # latlon | none (sem posição no mapa)
icon = "usina"                  # nome de tile de classe object no tileset, opcional
file = "data/usina.csv"         # colunas: entity,name,lat,lon

# Uma ou mais propriedades.
[[properties]]
id = "populacao"
entity_type = "municipio"       # base ou declarado (próprio ou de dependência, prefixado)
name = "População"
unit = "habitantes"
dtype = "i64"                   # i64 | f64 | str
periodicity = "anual"           # anual | decenal | mensal | diaria | pontual
source = "IBGE — Estimativas da população"
source_url = "https://www.ibge.gov.br/..."
license = "CC-BY-4.0"           # opcional, padrão = addon.license
file = "data/municipio_populacao.csv"

# Opcional: como exibir no mapa. Sem regra, a propriedade só aparece no painel.
[[render]]
property = "municipio/populacao"   # relativo ao próprio addon
kind = "choropleth"                # choropleth | icon | label
ramp = "viridis"                   # para choropleth
scale = "log"                      # linear | log

# Opcional: override manual de mapa.
[[map_overrides]]
level = 3
entity = "municipio:4314902"
path = "map/poa-centro"            # diretório com chunks .bmc; sem level.toml
chunks = [[8, 7], [9, 7]]          # quais (cx, cy) este override fornece
priority = 10                      # maior vence
```

## CSV de propriedade

Cabeçalho obrigatório, UTF-8, vírgula, ponto decimal:

```csv
entity,period_start,period_end,value
municipio:4314902,2022,2022,1332845
estado:43,2022,2022,10882965
```

- `period_start`/`period_end` em ISO 8601 (`YYYY`, `YYYY-MM` ou `YYYY-MM-DD`); `period_end`
  pode ficar vazio = igual a `period_start`.
- `value` conforme `dtype`. Vazio = ausente (linha ignorada com aviso).
- A chave `(entity, period_start, period_end)` é única dentro de um arquivo.

## Validação (o que o build do `bramapa-data` rejeita)

1. `spec_version` desconhecido.
2. `addon.id` inválido ou duplicado entre addons.
3. Propriedade ou tipo de entidade com `id` duplicado dentro do addon.
4. `entity_type` que não é base, nem declarado, nem vem de um addon em `depends`.
5. Arquivo referenciado inexistente; CSV sem as colunas exigidas; `value` fora do `dtype`.
6. Chave `(entity, period_start, period_end)` duplicada.
7. `render.property` ou `map_overrides.path` apontando para algo que não existe.
8. `depends` circular.

## O que um addon **não** pode fazer

Executar código, alterar a UI, redefinir propriedades de outro addon, substituir o tileset base.
Sobreposição de dados é impossível por construção: ids são prefixados pelo addon.

## Esclarecimentos v0 (decididos na Fase 0)

- **Regra 3 (ids duplicados)**: a chave de unicidade de uma propriedade dentro do addon é
  `(entity_type, id)`. `estado/populacao` e `pais/populacao` coexistem.
- **Referência a tipos de entidade**: tipos base e tipos declarados pelo próprio addon são
  referenciados sem prefixo (`usina`); tipos de uma dependência são sempre prefixados
  (`outro-addon/usina`). Um addon listado em `depends` que não existe no conjunto validado é erro.
- **Linhas do CSV**: toda linha de uma propriedade deve ter `entity` do tipo `entity_type` da
  propriedade. Códigos base: `pais:[A-Z]{2}`, `estado:\d{2}`, `municipio:\d{7}`.
- **Chave duplicada (regra 6)**: comparada após resolver os períodos em dias; `2022,2022`,
  `2022,` e `2022-01-01,2022-12-31` são a mesma chave.
- **Linhas em branco** são ignoradas; **`value` vazio** gera aviso e a linha é descartada antes
  da checagem de duplicidade.
- **CSV de tipo de entidade** (`entity_types[].file`): cabeçalho `entity,name` e, se
  `position = "latlon"`, mais `lat,lon` (graus decimais, validados). `entity` único.
- **`render`**: `kind = "choropleth"` exige `dtype` numérico; `ramp` ausente é aviso.
- **Diretório**: o nome do diretório deve ser igual a `addon.id` (divergência é aviso na v0).
- **Campos obrigatórios**:

  | Tabela | Obrigatórios | Opcionais |
  |---|---|---|
  | `[addon]` | `id`, `name`, `version` (semver), `license` | `authors`, `description`, `depends` |
  | `[[entity_types]]` | `id`, `name`, `min_level`, `position`, `file` | `icon` |
  | `[[properties]]` | `id`, `entity_type`, `name`, `unit`, `dtype`, `periodicity`, `source`, `file` | `source_url`, `license` |
  | `[[render]]` | `property`, `kind` | `ramp`, `scale` |
  | `[[map_overrides]]` | `level`, `entity`, `path` | `chunks`, `priority` |
