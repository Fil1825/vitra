import { X } from 'lucide-react';
import { motion } from 'motion/react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../../components/Glass/index.ts';
import { IconButton } from '../../components/ui/IconButton.tsx';
import { SegmentedControl, type SegmentedOption } from '../../components/ui/SegmentedControl.tsx';
import { Switch } from '../../components/ui/Switch.tsx';
import type {
  AppearanceSettings,
  Language,
  Preference,
  Switch as Toggle,
  WindowMaterial,
} from '../../lib/settings.ts';
import { useEnvironment } from '../../stores/environment.ts';
import { useSettings } from '../../stores/settings.ts';
import { Diagnostics } from './Diagnostics.tsx';
import styles from './SettingsPanel.module.css';
import { ThemePicker } from './ThemePicker.tsx';

export function SettingsPanel({ onClose }: { onClose(): void }) {
  const { t } = useTranslation();
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const supportsRefraction = useEnvironment((s) => s.supportsRefraction);
  const platform = useEnvironment((s) => s.platform);
  const vibrancy = useEnvironment((s) => s.vibrancy);
  const liquidGlassSupported = useEnvironment((s) => s.liquidGlassSupported);

  const setAppearance = (change: Partial<AppearanceSettings>) =>
    void update((current) => ({ ...current, appearance: { ...current.appearance, ...change } }));

  const preferences: { value: Preference; label: string }[] = [
    { value: 'system', label: t('settings.preference.system') },
    { value: 'reduce', label: t('settings.preference.reduce') },
    { value: 'full', label: t('settings.preference.full') },
  ];
  const materials: SegmentedOption<WindowMaterial>[] = [
    { value: 'vibrancy', label: t('settings.materials.vibrancy') },
    {
      value: 'liquidGlass',
      label: t('settings.materials.liquidGlass'),
      disabled: !liquidGlassSupported,
    },
  ];
  const materialHint = !liquidGlassSupported
    ? t('settings.liquidGlassUnsupported')
    : settings.appearance.windowMaterial === 'liquidGlass' && vibrancy !== 'liquidGlass'
      ? t('settings.liquidGlassFallback')
      : undefined;
  const toggles: { value: Toggle; label: string }[] = [
    { value: 'auto', label: t('settings.switch.auto') },
    { value: 'on', label: t('settings.switch.on') },
    { value: 'off', label: t('settings.switch.off') },
  ];
  const languages: { value: Language; label: string }[] = [
    { value: 'system', label: t('settings.languages.system') },
    { value: 'en', label: t('settings.languages.en') },
    { value: 'ru', label: t('settings.languages.ru') },
  ];

  return (
    // Only transform is animated: opacity on an ancestor would cut the glass off from the
    // backdrop it blurs (it becomes a "backdrop root").
    <motion.div
      className={styles.wrap}
      initial={{ x: '110%' }}
      animate={{ x: 0 }}
      exit={{ x: '110%' }}
      transition={{ type: 'spring', stiffness: 380, damping: 38 }}
    >
      <Glass as="aside" className={styles.panel} aria-label={t('settings.title')} priority={4}>
        <header className={styles.header}>
          <h2 className={styles.title}>{t('settings.title')}</h2>
          <IconButton label={t('settings.close')} onClick={onClose}>
            <X size={18} />
          </IconButton>
        </header>

        <div className={styles.content}>
          <section className={styles.section}>
            <h3 className={styles.sectionTitle}>{t('settings.appearance')}</h3>
            <ThemePicker
              value={settings.appearance.theme}
              onChange={(theme) => setAppearance({ theme })}
            />

            {/* Window materials are a macOS feature; elsewhere the backdrop is fixed. */}
            {platform === 'macos' && (
              <Row label={t('settings.windowMaterial')} hint={materialHint}>
                <SegmentedControl
                  label={t('settings.windowMaterial')}
                  value={settings.appearance.windowMaterial}
                  options={materials}
                  onChange={(windowMaterial) => setAppearance({ windowMaterial })}
                />
              </Row>
            )}

            <Row label={t('settings.motion')}>
              <SegmentedControl
                label={t('settings.motion')}
                value={settings.appearance.motion}
                options={preferences}
                onChange={(motion) => setAppearance({ motion })}
              />
            </Row>
            <Row label={t('settings.transparency')}>
              <SegmentedControl
                label={t('settings.transparency')}
                value={settings.appearance.transparency}
                options={preferences}
                onChange={(transparency) => setAppearance({ transparency })}
              />
            </Row>
            <Row
              label={t('settings.refraction')}
              hint={t(
                supportsRefraction
                  ? 'settings.refractionSupported'
                  : 'settings.refractionUnsupported',
              )}
            >
              <SegmentedControl
                label={t('settings.refraction')}
                value={settings.appearance.refraction}
                options={toggles}
                onChange={(refraction) => setAppearance({ refraction })}
              />
            </Row>
            <Row
              label={t('settings.artworkAccent')}
              hint={t('settings.artworkAccentHint')}
              layout="inline"
            >
              <Switch
                label={t('settings.artworkAccent')}
                checked={settings.appearance.artworkAccent}
                onChange={(artworkAccent) => setAppearance({ artworkAccent })}
              />
            </Row>
            <Row
              label={t('settings.powerSaving')}
              hint={t('settings.powerSavingHint')}
              layout="inline"
            >
              <Switch
                label={t('settings.powerSaving')}
                checked={settings.appearance.powerSaving}
                onChange={(powerSaving) => setAppearance({ powerSaving })}
              />
            </Row>
          </section>

          <section className={styles.section}>
            <h3 className={styles.sectionTitle}>{t('settings.language')}</h3>
            <SegmentedControl
              label={t('settings.language')}
              value={settings.language}
              options={languages}
              onChange={(language) => void update((current) => ({ ...current, language }))}
            />
          </section>

          <section className={styles.section}>
            <h3 className={styles.sectionTitle}>{t('settings.diagnostics')}</h3>
            <Diagnostics />
          </section>
        </div>
      </Glass>
    </motion.div>
  );
}

interface RowProps {
  label: string;
  hint?: string;
  /** `inline` puts the control next to the label (switches); `stacked` below it. */
  layout?: 'inline' | 'stacked';
  children: React.ReactNode;
}

function Row({ label, hint, layout = 'stacked', children }: RowProps) {
  return (
    <div className={layout === 'inline' ? styles.inlineRow : styles.row}>
      <div className={styles.rowText}>
        <span>{label}</span>
        {hint && <span className={styles.hint}>{hint}</span>}
      </div>
      {children}
    </div>
  );
}
