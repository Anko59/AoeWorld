import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["node_modules/**", "test-results/**"] },
  js.configs.recommended,
  ...tseslint.configs.strict,
  {
    // Each Playwright worker has its own game server: specs take `test` from
    // server.js, whose baseURL is that worker's server.
    files: ["tests/**/*.ts"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          paths: [
            {
              name: "@playwright/test",
              importNames: ["test"],
              message:
                "Import `test` from server.js so the spec uses its worker's own game server.",
            },
          ],
        },
      ],
    },
  },
);
