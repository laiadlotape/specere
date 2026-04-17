//! Telemetry entrypoint: `specere observe` is invoked from agent hooks and
//! emits OTel-shaped records to the locally scaffolded collector. In 0.1.0 this
//! is a stub; the full embedded receiver lands once `specere add otel-collector`
//! is implemented.

use specere_core::Ctx;

pub fn observe(_ctx: &Ctx) -> anyhow::Result<()> {
    tracing::warn!("`specere observe` is not yet implemented (planned in 0.2.0)");
    Ok(())
}
