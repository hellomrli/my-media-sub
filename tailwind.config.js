/** @type {import('tailwindcss').Config} */
module.exports = {
  darkMode: 'class',
  content: ['./static/index.html', './static/**/*.js'],
  theme: {
    extend: {
      colors: {
        app: 'rgb(var(--c-bg) / <alpha-value>)',
        surface: 'rgb(var(--c-surface) / <alpha-value>)',
        'surface-2': 'rgb(var(--c-surface-2) / <alpha-value>)',
        'surface-3': 'rgb(var(--c-surface-3) / <alpha-value>)',
        border: 'rgb(var(--c-border) / <alpha-value>)',
        text: 'rgb(var(--c-text) / <alpha-value>)',
        muted: 'rgb(var(--c-muted) / <alpha-value>)',
        primary: 'rgb(var(--c-primary) / <alpha-value>)',
        secondary: 'rgb(var(--c-secondary) / <alpha-value>)',
        success: 'rgb(var(--c-success) / <alpha-value>)',
        warning: 'rgb(var(--c-warning) / <alpha-value>)',
        danger: 'rgb(var(--c-danger) / <alpha-value>)',
        cyan: 'rgb(var(--c-cyan) / <alpha-value>)',
        violet: 'rgb(var(--c-violet) / <alpha-value>)',
      },
    },
  },
  plugins: [],
};
