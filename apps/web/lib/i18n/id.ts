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
  login: {
    help: "Butuh bantuan?",
    contactSupport: "Hubungi kami",
    eyebrow: "MASUK / DAFTAR",
    titlePhone: "Verifikasi nomor WhatsApp",
    subtitlePhone:
      "Kami kirim 6 digit kode ke nomor di bawah. Tanpa password, akun baru dibuat otomatis.",
    titleEmail: "Masuk dengan email",
    subtitleEmail:
      "Kami kirim 6 digit kode ke alamat di bawah. Tanpa password, akun baru dibuat otomatis.",
    phoneLabel: "Nomor WhatsApp",
    emailLabel: "Email",
    phonePlaceholder: "+62 812 3456 7890",
    emailPlaceholder: "kamu@usaha.id",
    sendCode: "Kirim kode",
    sending: "Mengirim…",
    codeTitle: "Masukkan kode",
    codeSubtitle: "6 digit sudah dikirim ke {contact}.",
    codeLabel: "Kode verifikasi",
    change: "Ganti",
    resendIn: "Kirim ulang dalam {seconds} dtk",
    resend: "Kirim ulang kode",
    verify: "Verifikasi & masuk",
    verifying: "Memverifikasi…",
    or: "atau",
    switchToEmail: "Masuk dengan email",
    switchToPhone: "Masuk dengan WhatsApp",
    terms: "Dengan masuk, kamu setuju Ketentuan Layanan & Kebijakan Privasi.",
    tenantTitle: "Pilih usaha",
    tenantSubtitle: "Akun ini terdaftar di lebih dari satu usaha.",
    invitationPending:
      "Kontak ini punya undangan yang belum diterima. Buka tautan undangan untuk bergabung.",
    steps: {
      contact: "Nomor",
      code: "Verifikasi",
      tenant: "Usaha",
    },
    errors: {
      invalidContact: "Masukkan nomor WhatsApp atau email yang valid.",
      invalidCode: "Kode salah atau sudah kedaluwarsa.",
      tooManyAttempts: "Terlalu banyak percobaan. Minta kode baru.",
      generic: "Ada yang salah. Coba lagi.",
    },
    foot: {
      platforms: "Android · iOS · Desktop",
      offline: "Bisa dipakai offline",
    },
    brand: {
      label: "UNTUK PRODUSEN KECIL YANG PRODUKSI PER BATCH",
      title: "Berhenti menghitung stok di buku tulis.",
      subtitle:
        "Takar mencatat batch, tanggal expired, resep, dan HPP dalam satu alur. Input dari HP, laporan di desktop, alert lewat WhatsApp.",
      features: {
        batch: {
          title: "Batch + FEFO",
          detail: "Batch expired terdekat dipakai lebih dulu, jadi tidak ada bahan terbuang diam-diam.",
        },
        hpp: {
          title: "HPP otomatis",
          detail: "Dihitung dari BOM × harga batch aktual, plus tenaga kerja dan overhead.",
        },
        alerts: {
          title: "Alert WhatsApp",
          detail: "H-7, H-3, dan stok minimum dikirim ke nomor tim setiap pagi.",
        },
      },
      noteTitle: "Data satu usaha tidak bercampur dengan usaha lain",
      noteSubtitle: "Row-level security per tenant · bisa dipakai offline di HP",
      version: "v1.0",
    },
  },
  dashboard: {
    title: "Ringkasan",
    greeting: "Masuk sebagai {name}",
    tenant: "Usaha",
    role: "Peran",
    permissions: "Hak akses",
    permissionsCount: "{count} hak akses",
    signOut: "Keluar",
    loading: "Memuat ruang kerja…",
    error: "Gagal memuat ruang kerja.",
    retry: "Coba lagi",
    roles: {
      owner: "Pemilik",
      admin: "Admin",
      production: "Produksi",
      viewer: "Pengamat",
    },
  },
};
