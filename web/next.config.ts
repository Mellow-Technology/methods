import type { NextConfig } from "next";

// In development the Next dev server proxies the API so the browser talks to
// one origin. In production the app is a static export the Rust server hosts.
const dev = process.env.NODE_ENV !== "production";
const api = process.env.METHODS_API_URL ?? "http://127.0.0.1:8080";

const config: NextConfig = {
  output: dev ? undefined : "export",
  trailingSlash: true,
  // API routes have no trailing slash; don't redirect them to one.
  skipTrailingSlashRedirect: true,
  transpilePackages: ["gui-mellow"],
  experimental: { optimizePackageImports: ["gui-mellow"] },
  ...(dev && {
    async rewrites() {
      return [
        { source: "/api/:path*", destination: `${api}/api/:path*` },
        { source: "/mcp", destination: `${api}/mcp` },
      ];
    },
  }),
};

export default config;
