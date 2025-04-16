use colored::Colorize;

pub fn colorize_provider(provider: &str) -> String {
    match provider {
        "cloudflare" => "Cloudflare"
            .to_string()
            .green()
            .truecolor(244, 129, 32)
            .to_string(),
        "porkbun" => "Porkbun"
            .to_string()
            .green()
            .truecolor(239, 120, 120)
            .to_string(),
        _ => provider.to_string(),
    }
}
