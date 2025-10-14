import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "export",
  trailingSlash: true,
  images: {
    unoptimized: true,
  },
  // Disable server-side features for static export
  experimental: {
    esmExternals: true,
  },
};

if (process.env.NODE_ENV === "development") {
  const rawBase =
    process.env.NEXT_PUBLIC_AGENTDEV_API_BASE ?? "http://localhost:3000";
  const normalizedBase = rawBase.replace(/\/$/, "");

  nextConfig.rewrites = async () => [
    {
      source: "/api/:path*",
      destination: `${normalizedBase}/api/:path*`,
    },
  ];
}

export default nextConfig;
