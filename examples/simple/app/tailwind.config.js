/** @type {import("tailwindcss").Config} */
module.exports = {
  // dont use overly-broad patterns like ./**/*.{js,css,html}
  // since this has no exclude option, unless they're all in 1 exclusive folder
  content: [
    "./index.html",
    "./404.html",
    "templates/**/*.html",
    "./css/**/*.css",
    "./ts/**/*.ts",
  ],
  plugins: [],
  darkMode: "class",
  theme: {
    screens: {
      sm: "640px",
      md: "768px",
      lg: "1024px",
      xl: "1280px",
      "2xl": "1536px",
    },
  },
};
