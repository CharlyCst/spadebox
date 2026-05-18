import { themes as prismThemes } from 'prism-react-renderer'
import type { Config } from '@docusaurus/types'
import type * as Preset from '@docusaurus/preset-classic'

const config: Config = {
  title: 'SpadeBox',
  tagline: 'Safe tools for AI agents',
  favicon: 'img/favicon.svg',

  future: {
    v4: true,
  },

  url: 'https://spadebox.dev',
  baseUrl: '/',

  organizationName: 'CharlyCst',
  projectName: 'spadebox',

  onBrokenLinks: 'throw',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    colorMode: {
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'SpadeBox',
      logo: {
        alt: 'SpadeBox Logo',
        src: 'img/spadebox-small-light-mode.svg',
        srcDark: 'img/spadebox-small-dark-mode.svg',
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'tutorialSidebar',
          position: 'left',
          label: 'Docs',
        },
        {
          href: 'https://github.com/CharlyCst/spadebox',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {
              label: 'Quick Start',
              to: '/docs/quick-start',
            },
          ],
        },
        {
          title: 'More',
          items: [
            {
              label: 'Blog',
              to: '/blog',
            },
            {
              label: 'GitHub',
              href: 'https://github.com/CharlyCst/spadebox',
            },
            {
              label: 'crates.io',
              href: 'https://crates.io/crates/spadebox-core',
            },
            {
              label: 'npm',
              href: 'https://www.npmjs.com/package/@spadebox/spadebox',
            },
            {
              label: 'PyPI',
              href: 'https://pypi.org/project/spadebox/',
            },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} SpadeBox. Built with Docusaurus.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['rust', 'toml', 'bash'],
    },
  } satisfies Preset.ThemeConfig,
}

export default config
