// Flat config (ESLint 10 dropped the legacy .eslintrc system). Mirrors the
// previous .eslintrc.cjs: eslint:recommended + @typescript-eslint/recommended +
// prettier, with our two custom rules.
import js from '@eslint/js';
import tseslint from '@typescript-eslint/eslint-plugin';
import prettier from 'eslint-config-prettier/flat';

export default [
  { ignores: ['dist', 'node_modules', 'coverage', 'playwright-report'] },
  js.configs.recommended,
  // [base (parser + plugin), eslint-recommended overrides, recommended rules]
  ...tseslint.configs['flat/recommended'],
  {
    languageOptions: { parserOptions: { ecmaVersion: 2022, sourceType: 'module' } },
    rules: {
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/consistent-type-imports': ['error', { fixStyle: 'inline-type-imports' }],
    },
  },
  prettier,
];
