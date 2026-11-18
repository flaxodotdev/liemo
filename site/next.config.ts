import type { NextConfig } from "next";

// A static site: `npm run build` writes plain HTML to out/, ready for any host.
const nextConfig: NextConfig = {
  output: "export",
  images: { unoptimized: true },
};

export default nextConfig;
