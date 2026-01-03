import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'msvc-kit',
  description: 'A portable MSVC Build Tools installer and manager',
  base: '/msvc-kit/',

  locales: {
    root: {
      label: 'English',
      lang: 'en',
    },
    zh: {
      label: '中文',
      lang: 'zh-CN',
      link: '/zh/',
    },
  },

  themeConfig: {
    logo: '/logo.svg',
    nav: [
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'API', link: '/api/library' },
      { text: 'Examples', link: '/examples/basic' },
      {
        text: 'DCC Integration',
        items: [
          { text: 'Unreal Engine 5', link: '/dcc/unreal-engine' },
          { text: 'Maya', link: '/dcc/maya' },
          { text: 'Houdini', link: '/dcc/houdini' },
          { text: '3ds Max', link: '/dcc/3dsmax' },
        ],
      },
    ],

    sidebar: {
      '/guide/': [
        {
          text: 'Introduction',
          items: [
            { text: 'What is msvc-kit?', link: '/guide/what-is-msvc-kit' },
            { text: 'Getting Started', link: '/guide/getting-started' },
            { text: 'Installation', link: '/guide/installation' },
          ],
        },
        {
          text: 'CLI Usage',
          items: [
            { text: 'Download', link: '/guide/cli-download' },
            { text: 'Setup Environment', link: '/guide/cli-setup' },
            { text: 'Configuration', link: '/guide/cli-config' },
            { text: 'List & Clean', link: '/guide/cli-list-clean' },
          ],
        },
        {
          text: 'Advanced',
          items: [
            { text: 'Caching Mechanism', link: '/guide/caching' },
            { text: 'Architecture Support', link: '/guide/architecture' },
            { text: 'CI/CD Integration', link: '/guide/ci-cd' },
          ],
        },
      ],
      '/api/': [
        {
          text: 'Library API',
          items: [
            { text: 'Overview', link: '/api/library' },
            { text: 'DownloadOptions', link: '/api/download-options' },
            { text: 'InstallInfo', link: '/api/install-info' },
            { text: 'MsvcEnvironment', link: '/api/msvc-environment' },
            { text: 'ToolPaths', link: '/api/tool-paths' },
          ],
        },
      ],
      '/examples/': [
        {
          text: 'Examples',
          items: [
            { text: 'Basic Usage', link: '/examples/basic' },
            { text: 'Custom Paths', link: '/examples/custom-paths' },
            { text: 'Build Script', link: '/examples/build-script' },
            { text: 'Quick Compile', link: '/examples/quick-compile' },
          ],
        },
      ],
      '/dcc/': [
        {
          text: 'DCC Integration',
          items: [
            { text: 'Overview', link: '/dcc/overview' },
            { text: 'Unreal Engine 5', link: '/dcc/unreal-engine' },
            { text: 'Maya', link: '/dcc/maya' },
            { text: 'Houdini', link: '/dcc/houdini' },
            { text: '3ds Max', link: '/dcc/3dsmax' },
            { text: 'Blender', link: '/dcc/blender' },
          ],
        },
      ],
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/loonghao/msvc-kit' },
    ],

    footer: {
      message: 'Released under the MIT License.',
      copyright: 'Copyright © 2024-present loonghao',
    },

    search: {
      provider: 'local',
    },
  },
})
