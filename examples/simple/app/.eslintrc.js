/** @type {import("eslint").ESLint.ConfigData} */
module.exports = {
  env: {
    browser: true,
  },
  ignorePatterns: ["dist/*", "templates/*", "vite.config.js"],
  overrides: [
    {
      files: ["*.js", "*.ts"],
      plugins: ["simple-import-sort"],
      extends: ["airbnb-base", "prettier"],
      parserOptions: {
        // support class field declarations
        ecmaVersion: "2022",
        sourceType: "module",
      },
      settings: {
        "import/resolver": {
          jsconfig: {
            config: "tsconfig.json",
          },
        },
      },
      rules: {
        "no-restricted-imports": "off",
        "import/ignore": "off",
        "import/extensions": "off", // allow `import X from "../node_modules/.../X.js"`
        "import/no-relative-packages": "off", // allow `import X from "../node_modules/..."`
        "import/prefer-default-export": "off",
        "no-restricted-syntax": [
          "error",
          {
            selector: "ForInStatement",
            message:
              "for..in loops iterate over the entire prototype chain, which is virtually never what you want. Use Object.{keys,values,entries}, and iterate over the resulting array.",
          },
          {
            selector: "LabeledStatement",
            message:
              "Labels are a form of GOTO; using them makes code confusing and hard to maintain and understand.",
          },
          {
            selector: "WithStatement",
            message:
              "`with` is disallowed in strict mode because it makes code impossible to predict and optimize.",
          },
        ],
        "no-plusplus": ["error", { allowForLoopAfterthoughts: true }],
        "no-param-reassign": ["error", { props: false }],
      },
    },
  ],
};
