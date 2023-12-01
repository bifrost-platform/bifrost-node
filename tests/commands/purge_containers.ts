import shell from 'shelljs';

async function purgeContainers() {
  shell.exec(`scripts/purge_docker_containers.sh`);
  console.log('[*] purged containers');
}

purgeContainers().catch(error => {
  console.log(error);
  process.exit(1);
});
