// ESLint flat config for the app, the website and the Node build scripts.
//
// Type-aware rules are on for TypeScript sources, so mistakes such as an
// un-awaited IPC promise are caught. Formatting is deliberately left out:
// the code base has a consistent hand style and no formatter is enforced.

import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: [
      "dist/**",
      "website/dist/**",
      "src-tauri/**",
      "node_modules/**",
      // Untracked local clone kept outside the build; never linted.
      "breathe/**",
    ],
  },

  js.configs.recommended,

  {
    files: ["**/*.{ts,tsx}"],
    extends: [tseslint.configs.recommendedTypeChecked],
    languageOptions: {
      parserOptions: {
        projectService: {
          // Vite configs belong to tsconfig.node.json, which the project
          // service does not discover on its own.
          allowDefaultProject: ["vite.config.ts", "website/vite.config.ts"],
        },
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },

  {
    rules: {
      // A leading underscore marks an intentionally unused binding.
      "no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", caughtErrorsIgnorePattern: "^_" },
      ],
    },
  },

  {
    files: ["**/*.{ts,tsx}"],
    rules: {
      "no-unused-vars": "off",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", caughtErrorsIgnorePattern: "^_" },
      ],
    },
  },

  {
    // Test doubles often stand in for async APIs without awaiting anything,
    // and save/restore globals such as window.matchMedia by reference.
    files: ["**/*.test.{ts,tsx}"],
    rules: {
      "@typescript-eslint/require-await": "off",
      "@typescript-eslint/unbound-method": "off",
    },
  },

  {
    files: ["src/**/*.{ts,tsx}", "website/src/**/*.{ts,tsx}"],
    extends: [reactHooks.configs.flat.recommended],
    languageOptions: {
      globals: globals.browser,
    },
  },

  {
    files: ["scripts/**/*.mjs", "*.config.{js,ts}", "website/*.config.ts"],
    languageOptions: {
      globals: globals.node,
    },
  },

  {
    files: ["public/**/*.js"],
    languageOptions: {
      sourceType: "script",
      globals: globals.browser,
    },
  },
);
