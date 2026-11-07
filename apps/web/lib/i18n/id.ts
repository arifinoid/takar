import type { Dictionary } from "./en";

export const id: Dictionary = {
  common: {
    appName: "Takar",
    tagline: "PPIC + POS untuk UMKM F&B",
    language: "Bahasa",
    localeId: "Bahasa Indonesia",
    localeEn: "English",
  },
  home: {
    eyebrow: "PPIC + POS ringan",
    lede: "Stok, tanggal expired, dan HPP produksi nyata untuk produsen F&B kecil.",
    cards: {
      web: {
        title: "Antarmuka web",
        detail: "Next.js static export, dipakai browser dan WebView Tauri.",
      },
      shell: {
        title: "Aplikasi native",
        detail: "Tauri 2 untuk desktop. Android menyusul setelah SDK siap.",
      },
      api: {
        title: "API",
        detail: "Layanan Axum dengan health check dan dokumen OpenAPI.",
      },
      core: {
        title: "Inti kalkulasi",
        detail: "Crate Rust murni untuk FEFO, HPP, dan konversi satuan.",
      },
    },
    footer: {
      phase: "Kerangka fase 0",
      api: "OpenAPI di /api-docs/openapi.json",
    },
  },
  health: {
    checking: "Memeriksa API…",
    offline: "API tidak terjangkau",
    online: "API {version}",
  },
};
