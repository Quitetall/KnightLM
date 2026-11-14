//! `blut-knightlm`: the KnightLM build of BLUT. Registers the KnightLM
//! cookbook plus the standard ML cookbook (so `.toml` recipes can name the
//! standard stages) and hands off to the BLUT CLI.

fn main() -> anyhow::Result<()> {
    // Before the runtime starts any thread (it sets an environment variable).
    blut_cookbook_knightlm::runctx::ensure_hf_runner_env();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let mut reg = blut_cookbook_knightlm::registry();
            reg.register(Box::new(blut_backends::StandardCookbook));
            blut::cli::run(reg).await
        })
}
