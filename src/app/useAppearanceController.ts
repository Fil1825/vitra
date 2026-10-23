import { useEffect, useLayoutEffect, useMemo } from 'react';

import { useGlassBudget } from '../components/Glass/budget.ts';
import { i18n, resolveLocale } from '../i18n/index.ts';
import { bus } from '../lib/bus.ts';
import { useMediaQuery } from '../lib/useMediaQuery.ts';
import { themeAssetUrl } from '../lib/themes.ts';
import { setWindowAppearance } from '../lib/window.ts';
import { useAppearance } from '../stores/appearance.ts';
import { useEnvironment } from '../stores/environment.ts';
import { usePlayer } from '../stores/player.ts';
import { useSettings } from '../stores/settings.ts';
import { useThemeEditor } from '../stores/themeEditor.ts';
import { useThemes } from '../stores/themes.ts';
import { accentFor } from '../theme/accent.ts';
import { applyTheme, applyThemeStyleSheet } from '../theme/apply.ts';
import { resolveEffects } from '../theme/effects.ts';
import { AUTO_THEME_ID, resolveTheme } from '../theme/themes.ts';
import { themeStyleSheet, type UserTheme } from '../theme/user.ts';

const onOff = (value: boolean) => (value ? 'on' : 'off');

/**
 * Keeps the document in sync with settings and the system: theme variables, effect flags on
 * `<html>` (read by CSS), the native window appearance and the UI language.
 */
export function useAppearanceController(): void {
  const appearance = useSettings((s) => s.settings.appearance);
  const language = useSettings((s) => s.settings.language);
  const { platform, vibrancy, supportsRefraction } = useEnvironment();

  const prefersDark = useMediaQuery('(prefers-color-scheme: dark)');
  const reducedMotion = useMediaQuery('(prefers-reduced-motion: reduce)');
  const reducedTransparency = useMediaQuery('(prefers-reduced-transparency: reduce)');

  const userThemes = useThemes((s) => s.user);
  const draft = useThemeEditor((s) => s.draft);
  // A theme being edited shows instead of the chosen one, so the whole window is its preview.
  const user: UserTheme | null = useMemo(
    () =>
      draft
        ? { theme: draft.theme, css: draft.css, assets: draft.assets }
        : (userThemes.find((t) => t.theme.id === appearance.theme) ?? null),
    [draft, userThemes, appearance.theme],
  );
  const theme = useMemo(
    () =>
      user?.theme ??
      resolveTheme(
        appearance.theme,
        prefersDark,
        userThemes.map((t) => t.theme),
      ),
    [user, appearance.theme, prefersDark, userThemes],
  );
  const styleSheet = useMemo(
    () => (user ? themeStyleSheet(user, (name) => themeAssetUrl(user.theme.id, name)).css : ''),
    [user],
  );
  const palette = usePlayer((s) => s.state.track?.artwork?.palette ?? null);
  const accent = useMemo(
    () =>
      theme.accentFromArtwork && appearance.artworkAccent && palette
        ? accentFor(palette.accent, theme.appearance)
        : null,
    [theme, appearance.artworkAccent, palette],
  );
  const effects = useMemo(
    () => resolveEffects({ reducedMotion, reducedTransparency }, appearance, supportsRefraction),
    [reducedMotion, reducedTransparency, appearance, supportsRefraction],
  );

  // Layout effects run before paint, so the first frame already has the theme.
  useLayoutEffect(() => {
    applyTheme(theme);
    useAppearance.setState({ theme });
    bus.emit('theme:changed', theme);
  }, [theme]);

  useLayoutEffect(() => applyThemeStyleSheet(styleSheet), [styleSheet]);

  // Runs after the theme effect, so it overrides (or restores) the theme's accent.
  useLayoutEffect(() => {
    const root = document.documentElement.style;
    root.setProperty('--vitra-color-accent', accent?.accent ?? theme.colors.accent);
    root.setProperty(
      '--vitra-color-accent-contrast',
      accent?.contrast ?? theme.colors.accentContrast,
    );
  }, [theme, accent]);

  useLayoutEffect(() => {
    const root = document.documentElement.dataset;
    root.motion = effects.motion ? 'full' : 'reduce';
    root.transparency = effects.transparency ? 'full' : 'reduce';
    root.backdropAnimation = onOff(effects.animatedBackdrop);
    useGlassBudget.getState().setMax(effects.maxGlassLayers);
    useAppearance.setState({ effects });
    bus.emit('effects:changed', effects);
  }, [effects]);

  useLayoutEffect(() => {
    const root = document.documentElement.dataset;
    root.platform = platform;
    root.vibrancy = onOff(vibrancy !== 'none');
  }, [platform, vibrancy]);

  const forcedAppearance = appearance.theme === AUTO_THEME_ID ? null : theme.appearance;
  useEffect(() => {
    setWindowAppearance(forcedAppearance).catch((err: unknown) =>
      console.error('Setting the window appearance failed', err),
    );
  }, [forcedAppearance]);

  useEffect(() => {
    void i18n.changeLanguage(resolveLocale(language, navigator.languages));
  }, [language]);
}
