const withNextra = require("nextra")({
  theme: "nextra-theme-docs",
  themeConfig: "./theme.config.tsx",
});

const nextConfig = {
  output: "export",
  basePath: "/docs",
  images: {
    unoptimized: true,
  },
};

module.exports = withNextra(nextConfig);
