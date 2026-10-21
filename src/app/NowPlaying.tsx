import { useTranslation } from 'react-i18next';

import symbolUrl from '../../assets/brand/symbol.svg';
import { Glass } from '../components/Glass/index.ts';
import styles from './NowPlaying.module.css';

/** Empty "now playing" card; artwork and track info arrive with the engine in stage 2. */
export function NowPlaying() {
  const { t } = useTranslation();
  return (
    <Glass as="section" className={styles.card} priority={1}>
      <img className={styles.symbol} src={symbolUrl} alt="" width={168} height={168} />
      <h2 className={styles.title}>{t('nowPlaying.emptyTitle')}</h2>
      <p className={styles.body}>{t('nowPlaying.emptyBody')}</p>
    </Glass>
  );
}
