/** @type {import('next').NextConfig} */
const nextConfig = {
  // Keep output tracing rooted at this app; the parent repository has its own
  // package-lock.json and must not be treated as this Next.js workspace.
  outputFileTracingRoot: __dirname,
};

module.exports = nextConfig;
