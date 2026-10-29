import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useGlassBudget } from '../../components/Glass/budget.ts';
import { getSidecarStatus, type SidecarStatus } from '../../lib/sidecars.ts';
import { useEnvironment } from '../../stores/environment.ts';
import styles from './SettingsPanel.module.css';

export function Diagnostics() {
  const { t } = useTranslation();
  const { platform, vibrancy } = useEnvironment();
  const layers = useGlassBudget((s) => s.slots.size);
  const max = useGlassBudget((s) => s.max);
  const [sidecars, setSidecars] = useState<SidecarStatus[]>([]);

  useEffect(() => {
    getSidecarStatus()
      .then(setSidecars)
      .catch((err: unknown) => console.error('Reading sidecar status failed', err));
  }, []);

  return (
    <dl className={styles.facts}>
      <dt>{t('settings.platform')}</dt>
      <dd>
        {platform === 'web'
          ? t('settings.browserPreview')
          : platform === 'macos'
            ? 'macOS'
            : platform === 'windows'
              ? 'Windows'
              : platform === 'linux'
                ? 'Linux'
                : platform}
      </dd>
      <dt>{t('settings.vibrancy')}</dt>
      <dd>{t(`settings.vibrancyKinds.${vibrancy}`)}</dd>
      <dt>{t('settings.glassLayers')}</dt>
      <dd>{t('settings.glassLayersValue', { active: Math.min(layers, max), max })}</dd>
      {sidecars.length > 0 && <dt className={styles.factsHeading}>{t('settings.sidecars')}</dt>}
      {sidecars.map((s) => (
        <dd key={s.name} className={styles.sidecar} title={s.path ?? undefined}>
          <span>{s.name}</span>
          <span className={s.path ? styles.path : styles.missing}>
            {s.path
              ? `${s.source ? t(`settings.sidecarSources.${s.source}`) : ''}: ${s.path}`
              : t('settings.notFound')}
          </span>
        </dd>
      ))}
    </dl>
  );
}
