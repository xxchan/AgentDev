import type { NextConfig } from "next";

const isDevelopment = process.env.NODE_ENV === "development";

const nextConfig: NextConfig = {
  trailingSlash: true,
  images: {
    unoptimized: true,
  },
  experimental: {
    esmExternals: true,
  },
  eslint: {
    ignoreDuringBuilds: true,
  },
  typescript: {
    ignoreBuildErrors: true,
  },
};

nextConfig.distDir = isDevelopment ? ".next-dev" : ".next-prod";

if (!isDevelopment) {
  nextConfig.output = "export";
}

if (isDevelopment) {
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
