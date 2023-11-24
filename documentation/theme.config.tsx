import React from 'react'

const config = {
  logo: (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 102.646 102.646" width={40} height={40} fill="none">
      <path stroke="currentColor" d="M101.323 51.323a50 50 0 01-50 50 50 50 0 01-50-50 50 50 0 0150-50 50 50 0 0150 50z"/>
      <path  stroke="currentColor" d="M35.78 43.327A13.63 14.433 0 0122.15 57.76 13.63 14.433 0 018.518 43.327a13.63 14.433 0 0113.63-14.432A13.63 14.433 0 0135.78 43.327zM58.075 1.309a12.294 10.69 0 01-3.525 11.59 12.294 10.69 0 01-13.684 1.457A12.294 10.69 0 0134.23 3.848M80.147 23.55a3.34 3.34 0 01-3.341 3.34 3.34 3.34 0 01-3.34-3.34 3.34 3.34 0 013.34-3.342 3.34 3.34 0 013.34 3.341M93.303 48.405a7.216 6.949 0 01-7.216 6.95 7.216 6.949 0 01-7.216-6.95 7.216 6.949 0 017.216-6.949 7.216 6.949 0 017.216 6.95zM78.677 92.75a23.119 21.035 0 00-13.235-19.015 23.119 21.035 0 00-24.55 2.756 23.119 21.035 0 00-7.76 21.371M13.694 84.223a7.108 8.698 0 005.575-8.513 7.108 8.698 0 00-5.607-8.482 7.108 8.698 0 00-7.969 4.89"/>
    </svg>
  ),
  logoLink:"https://lunatic.solutions/",
  docsRepositoryBase: "https://github.com/lunatic-solutions/lunatic",
  feedback: {
    content: 'Question? Give us feedback →',
    labels: 'feedback'
  },
  editLink: {
    text: ''
  },
  project: {
    link: 'https://github.com/lunatic-solutions/lunatic',
  },
  chat: {
    link: 'https://discord.gg/b7zDqpXpB4',
  },
  footer: {
    text: '© 2023, Lunatic Inc.',
  },
  useNextSeoProps() {
    return {
      titleTemplate: "Lunatic – %s",
    };
  }
}

export default config