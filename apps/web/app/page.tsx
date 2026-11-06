import { HealthBadge } from "./health-badge";

const foundations = [
  {
    title: "Web UI",
    detail: "Next.js static export, shared by the browser and the Tauri WebView.",
  },
  {
    title: "Native shell",
    detail: "Tauri 2 desktop build. Android follows once the SDK is available.",
  },
  {
    title: "API",
    detail: "Axum service with a health check and a generated OpenAPI document.",
  },
  {
    title: "Core",
    detail: "Pure Rust crate for FEFO, HPP, and unit conversion.",
  },
];

export default function HomePage() {
  return (
    <main className="page">
      <header className="header">
        <div>
          <p className="eyebrow">PPIC + light POS</p>
          <h1>Takar</h1>
          <p className="lede">
            Stock, expiry, and real production cost for small F&amp;B producers.
          </p>
        </div>
        <HealthBadge />
      </header>

      <section className="grid">
        {foundations.map((item) => (
          <article className="card" key={item.title}>
            <h2>{item.title}</h2>
            <p>{item.detail}</p>
          </article>
        ))}
      </section>

      <footer className="footer">
        <span>Phase 0 scaffold</span>
        <span>OpenAPI at /api-docs/openapi.json</span>
      </footer>
    </main>
  );
}
