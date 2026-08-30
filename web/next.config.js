/** @type {import('next').NextConfig} */
const nextConfig = {
  async rewrites() {
    return [{ source: "/api/:path*", destination: `${process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095"}/:path*` }];
  },
};
module.exports = nextConfig;
