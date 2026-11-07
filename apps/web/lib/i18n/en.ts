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
  login: {
    help: "Need help?",
    contactSupport: "Contact us",
    eyebrow: "SIGN IN / SIGN UP",
    titlePhone: "Verify your WhatsApp number",
    subtitlePhone:
      "We send a 6-digit code to the number below. No password, a new account is created automatically.",
    titleEmail: "Sign in with email",
    subtitleEmail:
      "We send a 6-digit code to the address below. No password, a new account is created automatically.",
    phoneLabel: "WhatsApp number",
    emailLabel: "Email",
    phonePlaceholder: "+62 812 3456 7890",
    emailPlaceholder: "you@business.id",
    sendCode: "Send code",
    sending: "Sending…",
    codeTitle: "Enter the code",
    codeSubtitle: "6 digits were sent to {contact}.",
    codeLabel: "Verification code",
    change: "Change",
    resendIn: "Resend in {seconds}s",
    resend: "Resend code",
    verify: "Verify and sign in",
    verifying: "Verifying…",
    or: "or",
    switchToEmail: "Sign in with email",
    switchToPhone: "Sign in with WhatsApp",
    terms: "By signing in you agree to the Terms of Service and Privacy Policy.",
    tenantTitle: "Choose a business",
    tenantSubtitle: "This account belongs to more than one business.",
    invitationPending:
      "This contact has a pending invitation. Open the invitation link to join that business.",
    steps: {
      contact: "Number",
      code: "Verification",
      tenant: "Business",
    },
    errors: {
      invalidContact: "Enter a valid WhatsApp number or email.",
      invalidCode: "That code is wrong or has expired.",
      tooManyAttempts: "Too many attempts. Request a new code.",
      generic: "Something went wrong. Try again.",
    },
    foot: {
      platforms: "Android · iOS · Desktop",
      offline: "Works offline",
    },
    brand: {
      label: "FOR SMALL PRODUCERS WHO WORK IN BATCHES",
      title: "Stop counting stock in a notebook.",
      subtitle:
        "Takar records batches, expiry dates, recipes, and HPP in one flow. Input from the phone, reports on the desktop, alerts through WhatsApp.",
      features: {
        batch: {
          title: "Batch + FEFO",
          detail: "The batch closest to expiry is used first, so no ingredient is quietly wasted.",
        },
        hpp: {
          title: "Automatic HPP",
          detail: "Calculated from the BOM times the actual batch price, plus labour and overhead.",
        },
        alerts: {
          title: "WhatsApp alerts",
          detail: "D-7, D-3, and minimum stock are sent to the team every morning.",
        },
      },
      noteTitle: "One business never sees another business's data",
      noteSubtitle: "Row-level security per tenant · works offline on the phone",
      version: "v1.0",
    },
  },
  dashboard: {
    title: "Overview",
    greeting: "Signed in as {name}",
    tenant: "Business",
    role: "Role",
    permissions: "Permissions",
    permissionsCount: "{count} permissions",
    signOut: "Sign out",
    loading: "Loading your workspace…",
    error: "Could not load your workspace.",
    retry: "Try again",
    roles: {
      owner: "Owner",
      admin: "Admin",
      production: "Production",
      viewer: "Viewer",
    },
  },
};

export type Dictionary = typeof en;
