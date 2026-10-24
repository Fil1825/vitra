import { LayoutDashboard } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { SegmentedControl } from '../../components/ui/SegmentedControl.tsx';
import { setCustomizing } from '../../layout/state.ts';
import { METER_STYLES, type MeterStyle } from '../../lib/settings.ts';
import { useSettings } from '../../stores/settings.ts';
import { Group, Row } from './Row.tsx';
import styles from './SettingsPanel.module.css';

/** Settings → Layout: the way into customize mode, and the level meter's look. */
export function LayoutSettings({ onClose }: { onClose(): void }) {
  const { t } = useTranslation();
  const meterStyle = useSettings((s) => s.settings.meter.style);
  const update = useSettings((s) => s.update);

  return (
    <>
      <Group>
        <Row label={t('layout.title')} hint={t('layout.hint')}>
          <button
            type="button"
            className={styles.textButton}
            onClick={() => {
              onClose();
              setCustomizing(true);
            }}
          >
            <LayoutDashboard size={14} aria-hidden="true" />
            {t('layout.customizeButton')}
          </button>
        </Row>
      </Group>
      <Group>
        <Row label={t('layout.spectrum')} hint={t('layout.spectrumHint')}>
          <SegmentedControl<MeterStyle>
            label={t('layout.spectrum')}
            value={meterStyle}
            options={METER_STYLES.map((style) => ({
              value: style,
              label: t(`layout.meterStyles.${style}`),
            }))}
            onChange={(style) =>
              void update((current) => ({ ...current, meter: { ...current.meter, style } }))
            }
          />
        </Row>
      </Group>
    </>
  );
}
