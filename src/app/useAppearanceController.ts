import { useEffect, useLayoutEffect, useMemo } from 'react';

import { useGlassBudget } from '../components/Glass/budget.ts';
import { i18n, resolveLocale } from '../i18n/index.ts';
import { bus } from '../lib/bus.ts';
import { useMediaQuery } from '../lib/useMediaQuery.ts';
import { setWindowAppearance } from '../lib/window.ts';
import { useAppearance } from '../stores/appearance.ts';
import { useEnvironment } from '../stores/environment.ts';
import { useSettings } from '../stores/settings.ts';
import { applyTheme } from '../theme/apply.ts';
import { resolveEffects } from '../theme/effects.ts';
import { AUTO_THEME_ID, resolveTheme } from '../theme/themes.ts';

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

  const theme = useMemo(
    () => resolveTheme(appearance.theme, prefersDark),
    [appearance.theme, prefersDark],
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
