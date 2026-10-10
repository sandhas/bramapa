#!/usr/bin/env bash
# Roda o cenário fixo de benchmark em release para cada nível de uma raiz e escreve docs/benchmark.md.
#   scripts/bench.sh <raiz-de-níveis> <tileset> [dados]
set -euo pipefail
ROOT=${1:?raiz de níveis}; TILESET=${2:?tileset}; DATA=${3:-}
OUT=docs/benchmark.md
cargo build --release -p bramapa-app
BIN=target/release/bramapa
{
  echo "# Benchmark"
  echo
  echo "Gerado por \`scripts/bench.sh\` em $(date -u +%Y-%m-%dT%H:%MZ) · host: $(uname -srm) · bramapa $(git describe --always --dirty)"
  echo
  echo "Cenário fixo (\`--bench\`): câmera no centro da grade, para cada zoom 1/32 … 2x, 30 quadros de"
  echo "aquecimento e 120 medidos, renderizando **fora da tela** em 1920×1080 (sem apresentar ao compositor) e esperando"
  echo "a GPU terminar cada quadro, então o tempo inclui CPU + GPU serializados e não sofre o teto do monitor."
  echo "Quadro médio e p99 em milissegundos; fps = 1000 / média. RSS medido no fim do cenário."
  echo
} > "$OUT"
for dir in "$ROOT"/1/* "$ROOT"/2/* "$ROOT"/3/*; do
  [ -f "$dir/level.toml" ] || continue
  args=(--level "$dir" --tileset "$TILESET" --bench "$OUT")
  [ -n "$DATA" ] && args+=(--data "$DATA")
  "$BIN" "${args[@]}" >/dev/null 2>&1
done
echo "escrito em $OUT"
