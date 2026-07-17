use crate::tui::app::LoadDialog;

pub(super) fn memory_summary(dialog: &LoadDialog) -> (f64, String) {
    let Some(memory) = dialog.memory.as_ref() else {
        return (0.0, "memory estimate unavailable".to_owned());
    };
    let ratio = memory.budget_bytes.filter(|budget| *budget > 0).map_or(0.0, |budget| {
        let basis_points =
            u16::try_from(u128::from(memory.required_bytes) * 10_000 / u128::from(budget))
                .unwrap_or(10_000)
                .min(10_000);
        f64::from(basis_points) / 10_000.0
    });
    let budget = memory.budget_bytes.map_or_else(|| "unknown".to_owned(), binary_size);
    let safe_context = memory
        .max_safe_context_tokens
        .map_or_else(|| "unknown".to_owned(), |tokens| tokens.to_string());
    let forced = if dialog.force {
        " · FORCE"
    } else {
        ""
    };
    (
        ratio,
        format!(
            "{} · need {} / budget {budget} · safe ctx {safe_context} · cache {}{forced}",
            memory.fit,
            binary_size(memory.required_bytes),
            memory.configured_cache_tokens,
        ),
    )
}

fn binary_size(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    let hundredths = bytes.saturating_mul(100) / GIB;
    format!("{}.{:02} GiB", hundredths / 100, hundredths % 100)
}
