// Source of truth for UI copy. Every other locale is typed against this
// object, so a missing or extra key is a compile error and fails the build.

export const en = {
  common: {
    appName: "Takar",
    tagline: "PPIC + light POS for F&B SMEs",
    language: "Language",
    localeId: "Bahasa Indonesia",
    localeEn: "English",
  },
  home: {
    eyebrow: "PPIC + light POS",
    lede: "Stock, expiry, and real production cost for small F&B producers.",
    cards: {
      web: {
        title: "Web UI",
        detail: "Next.js static export, shared by the browser and the Tauri WebView.",
      },
      shell: {
        title: "Native shell",
        detail: "Tauri 2 desktop build. Android follows once the SDK is available.",
      },
      api: {
        title: "API",
        detail: "Axum service with a health check and a generated OpenAPI document.",
      },
      core: {
        title: "Core",
        detail: "Pure Rust crate for FEFO, HPP, and unit conversion.",
      },
    },
    footer: {
      phase: "Phase 0 scaffold",
      api: "OpenAPI at /api-docs/openapi.json",
    },
  },
  health: {
    checking: "Checking API…",
    offline: "API offline",
    online: "API {version}",
  },
};

export type Dictionary = typeof en;
