//! Desktop entry point. Android uses `android_main` in `lib.rs`.

use std::path::PathBuf;

use bramapa_app::Options;

const USAGE: &str = "\
bramapa [opções]

  --level <dir>       diretório de nível (level.toml + chunks/*.bmc); padrão: nível sintético
  --levels <root>     raiz com <nível>/<entidade>/ (navegação: duplo clique/Enter entra, Backspace volta)
  --tileset <dir>     diretório de tileset (tiles.toml + atlas.png); padrão: atlas placeholder embutido
  --data <dir>        diretório de build de dados (index.toml + *.bmd) para o painel da entidade
  --size <w>x<h>      tamanho do nível sintético em células (padrão 512x512)
  --center <x>,<y>    célula inicial no centro da tela (padrão: centro da grade)
  --zoom <z>          zoom inicial: 1/32 … 1/2, 1 … 6 (padrão 1)
  --frames <n>        encerra após n quadros (para testes)
  --no-vsync          desliga o vsync (medição de performance)
  --bench <arquivo.md> roda o cenário fixo de benchmark (7 zooms × 120 quadros) e anexa a tabela
  --no-ui             inicia com o painel escondido (F1 mostra)
  --select <n>        inicia com a entidade de índice n selecionada (para testes)
  --enter             entra no nível da entidade selecionada logo após o primeiro quadro (testes)
  --fullscreen        janela em tela cheia sem borda
  --screenshot <png>  salva o último quadro em PNG (com --frames)
  -h, --help
";

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"),
    )
    .init();
    let mut opts = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--level" => {
                opts.level_dir =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        anyhow::anyhow!("--level precisa de valor")
                    })?))
            }
            "--levels" => {
                opts.levels_root =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        anyhow::anyhow!("--levels precisa de valor")
                    })?))
            }
            "--data" => {
                opts.data_dir = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--data precisa de valor"))?,
                ))
            }
            "--tileset" => {
                opts.tileset_dir =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        anyhow::anyhow!("--tileset precisa de valor")
                    })?))
            }
            "--size" => {
                let v = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--size precisa de valor"))?;
                let (w, h) = v
                    .split_once('x')
                    .ok_or_else(|| anyhow::anyhow!("--size: use WxH"))?;
                opts.synthetic_size = (w.parse()?, h.parse()?);
            }
            "--center" => {
                let v = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--center precisa de valor"))?;
                let (x, y) = v
                    .split_once(',')
                    .ok_or_else(|| anyhow::anyhow!("--center: use X,Y"))?;
                opts.center = Some((x.trim().parse()?, y.trim().parse()?));
            }
            "--zoom" => {
                let v = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--zoom precisa de valor"))?;
                opts.zoom = match v.split_once('/') {
                    Some((n, d)) => n.trim().parse::<f32>()? / d.trim().parse::<f32>()?,
                    None => v.parse()?,
                };
            }
            "--no-vsync" => opts.vsync = false,
            "--bench" => {
                opts.bench =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        anyhow::anyhow!("--bench precisa de valor")
                    })?));
                opts.vsync = false;
                opts.show_ui = false;
            }
            "--no-ui" => opts.show_ui = false,
            "--enter" => opts.enter_on_start = true,
            "--select" => {
                opts.select = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--select precisa de valor"))?
                    .parse()?
            }
            "--fullscreen" => opts.fullscreen = true,
            "--frames" => {
                opts.exit_after_frames = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--frames precisa de valor"))?
                        .parse()?,
                )
            }
            "--screenshot" => {
                opts.screenshot =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        anyhow::anyhow!("--screenshot precisa de valor")
                    })?))
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            other => anyhow::bail!("argumento desconhecido: {other}\n{USAGE}"),
        }
    }
    let event_loop = winit::event_loop::EventLoop::new()?;
    bramapa_app::run(event_loop, opts)
}
