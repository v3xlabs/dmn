use maud::PreEscaped;

pub const PORKBUN: PreEscaped<&str> = PreEscaped(include_str!("./public/porkbun_icon.svg"));
pub const CLOUDFLARE: PreEscaped<&str> = PreEscaped(include_str!("./public/cloudflare_icon.svg"));
pub const ENS: PreEscaped<&str> = PreEscaped(include_str!("./public/ens.svg"));

pub const GLOBE: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="1.35"/><path d="M2.4 8H13.6M8 2C9.7 3.6 10.6 5.7 10.6 8C10.6 10.3 9.7 12.4 8 14C6.3 12.4 5.4 10.3 5.4 8C5.4 5.7 6.3 3.6 8 2Z" stroke="currentColor" stroke-width="1.25"/></svg>"#,
);

pub const ALL_PROVIDERS: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M8 2.25L13.75 5.25L8 8.25L2.25 5.25L8 2.25Z" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round"/><path d="M2.25 8.25L8 11.25L13.75 8.25M2.25 11.25L8 14.25L13.75 11.25" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
);

pub const BRAND_MARK: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M3.5 8H12.5M8 3.5V12.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/><circle cx="8" cy="8" r="2.1" fill="currentColor"/></svg>"#,
);

pub const CALENDAR: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><rect x="2" y="3" width="12" height="11" rx="2" stroke="currentColor" stroke-width="1.4"/><path d="M5 1.75V4M11 1.75V4M2 6.5H14" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>"#,
);

pub const RSS: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><circle cx="3.75" cy="12.25" r="1.25" fill="currentColor"/><path d="M2.75 7.5A5.75 5.75 0 0 1 8.5 13.25M2.75 3.25A10 10 0 0 1 12.75 13.25" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>"#,
);

pub const DOCS: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M2.5 3.5H6.5A2 2 0 0 1 8.5 5.5V13.25A1.5 1.5 0 0 0 7 11.75H2.5V3.5ZM13.5 3.5H9.5A2 2 0 0 0 7.5 5.5V13.25A1.5 1.5 0 0 1 9 11.75H13.5V3.5Z" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round"/></svg>"#,
);

pub const SUN: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><circle cx="8" cy="8" r="3" stroke="currentColor" stroke-width="1.4"/><path d="M8 1.75V3.25M8 12.75V14.25M1.75 8H3.25M12.75 8H14.25M3.6 3.6L4.65 4.65M11.35 11.35L12.4 12.4M12.4 3.6L11.35 4.65M4.65 11.35L3.6 12.4" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>"#,
);

pub const MOON: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M13.25 9.85A5.5 5.5 0 0 1 6.15 2.75A5.75 5.75 0 1 0 13.25 9.85Z" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/></svg>"#,
);

pub const REFRESH: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M2.5 8a5.5 5.5 0 0 1 9.35-3.93L13.5 5.5M13.5 2.5v3h-3M13.5 8a5.5 5.5 0 0 1-9.35 3.93L2.5 10.5M2.5 13.5v-3h3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
);

pub const SEARCH: PreEscaped<&str> = PreEscaped(
    r#"<svg class="search-icon" viewBox="0 0 16 16" fill="none" aria-hidden="true"><circle cx="7.25" cy="7.25" r="4.75" stroke="currentColor" stroke-width="1.4"/><path d="M10.75 10.75L14 14" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>"#,
);

pub const RENEW: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M2.75 8a5.25 5.25 0 0 1 8.95-3.72L13.25 5.75M13.25 2.75v3h-3M13.25 8a5.25 5.25 0 0 1-8.95 3.72L2.75 10.25M2.75 13.25v-3h3" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
);

pub const PRIVACY_ON: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M8 2.25L13 4.25V7.75C13 10.8 10.9 13.1 8 14C5.1 13.1 3 10.8 3 7.75V4.25L8 2.25Z" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round"/><path d="M5.9 8.1L7.3 9.5L10.3 6.5" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
);

pub const PRIVACY_EXPOSED: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M1.75 8C3 5.4 5.2 4 8 4C10.8 4 13 5.4 14.25 8C13 10.6 10.8 12 8 12C5.2 12 3 10.6 1.75 8Z" stroke="currentColor" stroke-width="1.35"/><circle cx="8" cy="8" r="1.8" stroke="currentColor" stroke-width="1.35"/></svg>"#,
);

pub const CHEVRON_RIGHT: PreEscaped<&str> = PreEscaped(
    r#"<svg class="row-chevron" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M6 3.75L10.25 8L6 12.25" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
);

pub const CHEVRON_DOWN: PreEscaped<&str> = PreEscaped(
    r#"<svg class="provider-caret" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M4.25 6.25L8 10L11.75 6.25" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
);

pub const CHECK: PreEscaped<&str> = PreEscaped(
    r#"<svg class="provider-check" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M3.5 8.5L6.5 11.5L12.5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
);

pub const SORT: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M5 6L8 3L11 6M5 10L8 13L11 10" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
);

pub const ARROW_UP: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M4.5 9.5L8 6L11.5 9.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
);

pub const ARROW_DOWN: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M4.5 6.5L8 10L11.5 6.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
);

pub const CLOSE: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M4.5 4.5L11.5 11.5M11.5 4.5L4.5 11.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>"#,
);

pub const COPY: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5" stroke="currentColor" stroke-width="1.35"/><path d="M10.5 5.5V3.75A1.25 1.25 0 0 0 9.25 2.5H3.75A1.25 1.25 0 0 0 2.5 3.75V9.25A1.25 1.25 0 0 0 3.75 10.5H5.5" stroke="currentColor" stroke-width="1.35"/></svg>"#,
);

pub const CODE: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M5.5 4.5L2 8L5.5 11.5M10.5 4.5L14 8L10.5 11.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
);

pub const EXTERNAL: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 14 14" fill="none" aria-hidden="true"><path d="M5.25 2.75H11.25V8.75M11 3L3.25 10.75" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#,
);
