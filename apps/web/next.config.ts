import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // The UI is loaded by the browser and by the Tauri WebView from static files.
  output: "export",
  images: {
    unoptimized: true,
  },
  trailingSlash: true,
  transpilePackages: ["@takar/api-client"],
};

export default nextConfig;
