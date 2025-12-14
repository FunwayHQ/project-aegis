/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        'aegis-teal': '#14b8a6',
        'aegis-dark': '#1a1a2e',
        'darkGrey': '#1f2937',
      },
    },
  },
  plugins: [],
}
