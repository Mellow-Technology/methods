import type { NextConfig } from "next";

// The landing page, published to GitHub Pages. Pages serves it from a subpath
// (/methods); the deploy workflow sets this from the Pages configuration.
const basePath = process.env.PAGES_BASE_PATH ?? "";

const config: NextConfig = {
  output: "export",
  basePath,
  trailingSlash: true,
  transpilePackages: ["gui-mellow"],
  experimental: { optimizePackageImports: ["gui-mellow"] },
};

export default config;
