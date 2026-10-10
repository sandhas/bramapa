# Contribuindo com o Bramapa

O Bramapa é um framework: o motor é pequeno e estável, e o valor vem do que a comunidade adiciona.
Há quatro formas de contribuir, em ordem de facilidade. Nenhuma exige saber Rust, exceto a última.

## 1. Dados: um addon

Um addon é um diretório com um `manifest.toml` e CSVs. **Nunca contém código.** Ele declara
propriedades (ex.: `energia/estado/tarifa_residencial`) com unidade, fonte, licença e série
temporal, e o motor as mostra no painel da entidade. Dois addons nunca se sobrepõem: o id do
addon é o namespace de tudo que ele declara.

- Spec: [`docs/spec/addon.md`](spec/addon.md).
- Modelo: [`bramapa-data/addons/_template`](https://github.com/sandhas/bramapa-data/tree/main/addons/_template).
- Pode viver no repositório `bramapa-data` (via pull request) ou **no seu próprio repositório**,
  como o [`bramapa-addon-energia`](https://github.com/sandhas/bramapa-addon-energia). Nos dois
  casos o fluxo é o mesmo:

```sh
cd bramapa-data
uv run bramapa-data validate ../meu-addon/addons        # zero erros
uv run bramapa-data build addons ../meu-addon/addons --out ../bramapa-data-build
```

Regras que valem a pena saber antes de começar: ids de entidade são os do IBGE
(`estado:43`, `municipio:4314902`), períodos são ISO 8601 (`2022`, `2022-03`, `2022-03-15`),
e a licença dos dados é declarada no manifesto (dados do IBGE são CC-BY; ANEEL e EPE têm as suas).
Commite também o script que buscou os dados, para a série poder ser atualizada.

## 2. Mapa: um override manual

Se o mapa procedural ficou feio num lugar que você conhece, um addon pode substituir chunks
inteiros por chunks desenhados. Exemplo: [`poa-centro-exemplo`](https://github.com/sandhas/bramapa-data/tree/main/addons/poa-centro-exemplo),
que troca o centro de Porto Alegre por uma praça. O chunk de override deve manter a camada de
entidade do original (a ferramenta `tools/make_override.py` faz isso), e o manifesto declara
`level`, `entity`, `chunks` e `priority`. O motor aplica ao carregar; maior prioridade vence.

## 3. Arte: tiles

O tileset de hoje é um placeholder gerado por código. Arte de verdade entra em
[`bramapa-tileset`](https://github.com/sandhas/bramapa-tileset): losango de 64×32 em cells de
64×64, âncora no centro inferior, animação por frames consecutivos, variantes de autotiling por
máscara de vizinhança. Spec em [`docs/spec/tileset.md`](spec/tileset.md). O `check` do repo
valida tudo; comece substituindo uma cobertura (por exemplo, `floresta`) e veja no app.

## 4. Pipeline e motor

- [`bramapa-pipeline`](https://github.com/sandhas/bramapa-pipeline) (Python): novas fontes de
  geodado, classes de cobertura, níveis para outros estados e municípios. Pontos abertos:
  extratos do Geofabrik no lugar do Overpass, autotiling de estradas, capitais como objetos.
- [`bramapa`](https://github.com/sandhas/bramapa) (Rust): renderer, câmera, UI. Critério 1 é
  performance: cada mudança no caminho quente precisa de uma linha no
  [`docs/benchmark.md`](benchmark.md) (`scripts/bench.sh`).

## Regras comuns

- Issues e PRs em português ou inglês. Código e comentários em inglês; docs em português.
- Toda mudança de formato passa pela spec em `docs/spec/` primeiro, com `spec_version`.
- Licenças: código MIT ou Apache-2.0; dados e arte com a licença declarada no manifesto ou no
  tileset. OpenStreetMap exige atribuição (ODbL); MapBiomas é CC-BY-SA.
- Teste local antes do PR: `cargo test`, `uv run pytest`, `bramapa-data validate`,
  `bramapa-tileset check`, conforme o repo.
