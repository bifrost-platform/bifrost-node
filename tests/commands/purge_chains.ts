import shell from 'shelljs';

/**
 * purge chain data
 */
async function purgeChains() {
  // purge_chains.sh
  shell.exec(`scripts/purge_chains.sh`);
  console.log('[*] purged chains');
}
purgeChains().catch(error => {
  console.error(error);
  process.exit(1);
});
