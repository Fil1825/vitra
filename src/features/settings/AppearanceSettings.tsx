import { useTranslation } from 'react-i18next';

import { SegmentedControl, type SegmentedOption } from '../../components/ui/SegmentedControl.tsx';
import { Switch } from '../../components/ui/Switch.tsx';
import type {
  AppearanceSettings as Appearance,
  Preference,
  Switch as Toggle,
  WindowMaterial,
} from '../../lib/settings.ts';
import { useEnvironment } from '../../stores/environment.ts';
import { useSettings } from '../../stores/settings.ts';
import { Group, Row } from './Row.tsx';
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
