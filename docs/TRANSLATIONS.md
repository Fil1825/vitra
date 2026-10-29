# English and Russian

The catalogs are `src/i18n/locales/en.json` and `ru.json`. English is the fallback. The
system language selects the first supported language unless Settings → General explicitly
chooses English or Russian. The mini player and native menu labels follow the same setting.

Release preparation audited the component text and both catalogs. Visible boot failures
now use the system language even if startup failed before i18next initialized. Helper
locations in Diagnostics are translated. Russian download-error counts include all plural
forms. Technical exception details and helper-program output retain their original text;
do not translate paths, song metadata, or upstream diagnostic messages.

```sh
npm test -- src/i18n
```

The tests verify equal logical keys, nonempty values, matching interpolation parameters,
the plural categories required by each locale, and actual rendering of every plural family
for zero, singular, and several plural counts. A component-source check catches untranslated
plain JSX text and literal title, placeholder, alternative-text, and accessibility labels.
Brand names are explicitly exempt. Dynamic strings still require review.

For a new label, add both translations and call `t(...)`; never concatenate a count and a
noun. Preserve named interpolation parameters in every translation and provide each form
required by `Intl.PluralRules`. Use existing terminology consistently: My Wave / Моя волна,
queue / очередь, library / библиотека.

Before a release, switch languages while the main window, mini player, settings, context
menus, import dialog, and theme editor are open. Check long Russian labels, keyboard focus,
and reduced motion. These visual checks complement automated catalog checks.
