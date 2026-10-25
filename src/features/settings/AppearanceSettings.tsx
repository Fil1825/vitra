import { RotateCcw } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  formatSpeed,
  previewVinylSpeed,
  SpeedDial,
  useVinyl,
} from '../../components/CoverArt/index.ts';
import { IconButton } from '../../components/ui/IconButton.tsx';
import { SegmentedControl, type SegmentedOption } from '../../components/ui/SegmentedControl.tsx';
import { Switch } from '../../components/ui/Switch.tsx';
import type {
  AppearanceSettings as Appearance,
  Preference,
  Switch as Toggle,
  WindowMaterial,
} from '../../lib/settings.ts';
import { useAppearance } from '../../stores/appearance.ts';
import { useEnvironment } from '../../stores/environment.ts';
import { useSettings } from '../../stores/settings.ts';
import { vinylSpeed } from '../../theme/artwork.ts';
import { Group, Row } from './Row.tsx';
import styles from './SettingsPanel.module.css';
import { ThemePicker } from './ThemePicker.tsx';

/** Settings → Appearance: the theme, the window's glass, motion and covers. */
export function AppearanceSettings() {
  const { t } = useTranslation();
  const appearance = useSettings((s) => s.settings.appearance);
  const update = useSettings((s) => s.update);
  const supportsRefraction = useEnvironment((s) => s.supportsRefraction);
  const platform = useEnvironment((s) => s.platform);
  const vibrancy = useEnvironment((s) => s.vibrancy);
  const liquidGlassSupported = useEnvironment((s) => s.liquidGlassSupported);

  const set = (change: Partial<Appearance>) =>
    void update((current) => ({ ...current, appearance: { ...current.appearance, ...change } }));

  const preferences: SegmentedOption<Preference>[] = [
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
    : appearance.windowMaterial === 'liquidGlass' && vibrancy !== 'liquidGlass'
      ? t('settings.liquidGlassFallback')
      : undefined;
  const toggles: SegmentedOption<Toggle>[] = [
    { value: 'auto', label: t('settings.switch.auto') },
    { value: 'on', label: t('settings.switch.on') },
    { value: 'off', label: t('settings.switch.off') },
  ];

  return (
    <>
      <Group title={t('settings.theme')}>
        <ThemePicker value={appearance.theme} onChange={(theme) => set({ theme })} />
      </Group>

      <Group title={t('settings.groups.window')}>
        {/* Window materials are a macOS feature; elsewhere the backdrop is fixed. */}
        {platform === 'macos' && (
          <Row label={t('settings.windowMaterial')} hint={materialHint}>
            <SegmentedControl
              label={t('settings.windowMaterial')}
              value={appearance.windowMaterial}
              options={materials}
              onChange={(windowMaterial) => set({ windowMaterial })}
            />
          </Row>
        )}
        <Row label={t('settings.transparency')}>
          <SegmentedControl
            label={t('settings.transparency')}
            value={appearance.transparency}
            options={preferences}
            onChange={(transparency) => set({ transparency })}
          />
        </Row>
        <Row
          label={t('settings.refraction')}
          hint={t(
            supportsRefraction ? 'settings.refractionSupported' : 'settings.refractionUnsupported',
          )}
        >
          <SegmentedControl
            label={t('settings.refraction')}
            value={appearance.refraction}
            options={toggles}
            onChange={(refraction) => set({ refraction })}
          />
        </Row>
      </Group>

      <Group title={t('settings.groups.motion')}>
        <Row label={t('settings.motion')}>
          <SegmentedControl
            label={t('settings.motion')}
            value={appearance.motion}
            options={preferences}
            onChange={(motion) => set({ motion })}
          />
        </Row>
        <Row
          label={t('settings.backdropPulse')}
          hint={t('settings.backdropPulseHint')}
          layout="inline"
        >
          <Switch
            label={t('settings.backdropPulse')}
            checked={appearance.backdropPulse}
            onChange={(backdropPulse) => set({ backdropPulse })}
          />
        </Row>
        <Row label={t('settings.powerSaving')} hint={t('settings.powerSavingHint')} layout="inline">
          <Switch
            label={t('settings.powerSaving')}
            checked={appearance.powerSaving}
            onChange={(powerSaving) => set({ powerSaving })}
          />
        </Row>
      </Group>

      <Group title={t('settings.groups.covers')}>
        <Row label={t('settings.vinyl')} hint={t('settings.vinylHint')}>
          <SegmentedControl
            label={t('settings.vinyl')}
            value={appearance.vinyl}
            options={[
              { value: 'auto', label: t('settings.vinylAuto') },
              { value: 'on', label: t('settings.switch.on') },
              { value: 'off', label: t('settings.switch.off') },
            ]}
            onChange={(vinyl) => set({ vinyl })}
          />
        </Row>
        <Row label={t('settings.vinylSpeed')} hint={t('settings.vinylSpeedHint')}>
          <VinylSpeed />
        </Row>
        <Row
          label={t('settings.artworkAccent')}
          hint={t('settings.artworkAccentHint')}
          layout="inline"
        >
          <Switch
            label={t('settings.artworkAccent')}
            checked={appearance.artworkAccent}
            onChange={(artworkAccent) => set({ artworkAccent })}
          />
        </Row>
      </Group>
    </>
  );
}

/**
 * How fast records turn: tried out live while the slider moves (the spin clock reads the
 * preview), saved when it is let go. A speed equal to the theme's (else 33⅓) is no choice
 * of the user's: it is saved as none, so the theme's speed applies again.
 */
function VinylSpeed() {
  const { t, i18n } = useTranslation();
  const setting = useSettings((s) => s.settings.appearance.vinylSpeed);
  const update = useSettings((s) => s.update);
  const theme = useAppearance((s) => s.theme);
  const records = useVinyl();
  const [dragged, setDragged] = useState<number | null>(null);
  const usual = vinylSpeed(theme, null);

  // Leaving the page mid-drag doesn't leave the records at the tried speed.
  useEffect(() => () => previewVinylSpeed(null), []);

  const save = (speed: number | null) =>
    void update((current) => ({
      ...current,
      appearance: {
        ...current.appearance,
        vinylSpeed: speed === null || Math.abs(speed - usual) < 0.01 ? null : speed,
      },
    }));

  return (
    <div className={styles.speed}>
      <SpeedDial
        label={t('settings.vinylSpeed')}
        speed={dragged ?? vinylSpeed(theme, setting)}
        disabled={!records}
        onChange={(speed) => {
          setDragged(speed);
          previewVinylSpeed(speed);
        }}
        onCommit={(speed) => {
          setDragged(null);
          previewVinylSpeed(null);
          save(speed);
        }}
      />
      <IconButton
        label={t('settings.vinylSpeedReset', { value: formatSpeed(usual, i18n.language) })}
        disabled={setting === null || !records}
        onClick={() => save(null)}
      >
        <RotateCcw size={14} />
      </IconButton>
    </div>
  );
}
