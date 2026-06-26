/** @type {import('tailwindcss').Config} */
module.exports = {
  darkMode: 'class',
  content: ['./static/index.html', './static/app.js'],
  theme: {
    extend: {
      colors: {
        dark: { bg: '#08111f', card: '#111d2c', border: '#284159', hover: '#162638' },
      },
    },
  },
  plugins: [],
};
