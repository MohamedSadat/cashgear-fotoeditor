import { createHash, randomBytes } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createReadStream } from 'node:fs';
import { cp, mkdir, mkdtemp, readFile, statfs, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { root, run } from './lib.mjs';

// Microsoft Windows 10 Enterprise Evaluation 22H2 x64 English (US).
const media =
  'https://software-static.download.prss.microsoft.com/dbazure/988969d5-f34g-4e03-ac9d-1f9786c66750/' +
  '19045.2006.220908-0225.22h2_release_svc_refresh_CLIENTENTERPRISEEVAL_OEMRET_x64FRE_en-us.iso';
const mediaSha256 = '4f8367c4db803e712ed26689a0106aba1206513f6a6e35ad714f446711051667';
const output = join(root, 'bench/results/clean-windows-10');
const packages = join(root, 'bench/results/desktop-acceptance');
const vmRoot = tmpdir();
const space = await statfs(vmRoot);
if (space.bavail * space.bsize < 20 * 1024 ** 3)
  throw new Error('The hosted runner needs at least 20 GiB free on its temporary disk.');
const scratch = await mkdtemp(join(vmRoot, 'fotoeditor-'));
const image = join(scratch, 'windows10.iso');
const qaMedia = join(scratch, 'qa-media');
const qaFiles = join(qaMedia, 'files');
const qaIso = join(scratch, 'answer.iso');
const disk = join(scratch, 'windows.qcow2');
await mkdir(output, { recursive: true });
await mkdir(qaFiles, { recursive: true });

const download = (url, destination) =>
  run('curl', [
    '--fail',
    '--silent',
    '--show-error',
    '--location',
    '--retry',
    '3',
    '--output',
    destination,
    url,
  ]);
async function digest(file) {
  const hash = createHash('sha256');
  for await (const bytes of createReadStream(file)) hash.update(bytes);
  return hash.digest('hex');
}

const version = process.version;
const nodeName = 'node-' + version + '-win-x64.zip';
const nodeZip = join(qaMedia, 'node.zip');
const checksums = join(scratch, 'SHASUMS256.txt');
await Promise.all([
  download(media, image),
  download(`https://nodejs.org/dist/${version}/${nodeName}`, nodeZip),
  download(`https://nodejs.org/dist/${version}/SHASUMS256.txt`, checksums),
]);
const nodeHash = (await readFile(checksums, 'utf8'))
  .split('\n')
  .find((line) => line.endsWith('  ' + nodeName))
  ?.split(' ')[0];
if ((await digest(image)) !== mediaSha256 || (await digest(nodeZip)) !== nodeHash)
  throw new Error('Windows or Node download checksum mismatch.');

for (const name of [
  'baseline.exe',
  'CashGearFotoEditor-Setup-x64.exe',
  'latest.json',
  'source.cgfoto',
]) {
  await cp(join(packages, name), join(qaFiles, 'assets', name));
}
for (const name of [
  'scripts/desktop-package-smoke.mjs',
  'scripts/desktop-release.mjs',
  'scripts/lib.mjs',
  'apps/desktop/package.json',
]) {
  await cp(join(root, name), join(qaFiles, name));
}
let require = createRequire(import.meta.url);
for (const name of ['@playwright/test', 'playwright', 'playwright-core']) {
  const source = require.resolve(name + '/package.json');
  await cp(dirname(source), join(qaFiles, 'node_modules', name), {
    recursive: true,
    dereference: true,
  });
  require = createRequire(source);
}

const incoming = await mkdtemp(join(tmpdir(), 'fotoeditor-guest-'));
let result;
const server = createServer(async (request, response) => {
  if (request.method !== 'PUT' || !['/verification.json', '/guest.log'].includes(request.url)) {
    response.writeHead(404).end();
    return;
  }
  const chunks = [];
  let size = 0;
  for await (const part of request) {
    size += part.length;
    if (size > 1024 ** 2) {
      response.writeHead(413).end();
      return;
    }
    chunks.push(part);
  }
  const name = request.url.slice(1);
  await writeFile(join(incoming, name), Buffer.concat(chunks));
  response.end('saved');
  if (name === 'verification.json') result = JSON.parse(Buffer.concat(chunks).toString('utf8'));
});
await new Promise((resolve, reject) => server.listen(0, '127.0.0.1', resolve).on('error', reject));
const port = server.address().port;
const password = randomBytes(12).toString('hex') + 'G4!';
const ns = 'xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State"';
const component = (name) =>
  `<component name="${name}" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" ${ns}>`;
await writeFile(
  join(qaMedia, 'Autounattend.xml'),
  `<?xml version="1.0" encoding="utf-8"?>
<unattend xmlns="urn:schemas-microsoft-com:unattend">
  <settings pass="windowsPE">
    ${component('Microsoft-Windows-International-Core-WinPE')}
      <SetupUILanguage><UILanguage>en-US</UILanguage></SetupUILanguage>
      <InputLocale>en-US</InputLocale><SystemLocale>en-US</SystemLocale><UILanguage>en-US</UILanguage><UserLocale>en-US</UserLocale>
    </component>
    ${component('Microsoft-Windows-Setup')}
      <DiskConfiguration>
        <Disk wcm:action="add"><DiskID>0</DiskID><WillWipeDisk>true</WillWipeDisk>
          <CreatePartitions><CreatePartition wcm:action="add"><Order>1</Order><Type>Primary</Type><Extend>true</Extend></CreatePartition></CreatePartitions>
          <ModifyPartitions><ModifyPartition wcm:action="add"><Order>1</Order><PartitionID>1</PartitionID><Format>NTFS</Format><Active>true</Active><Label>Windows</Label><Letter>C</Letter></ModifyPartition></ModifyPartitions>
        </Disk>
        <WillShowUI>OnError</WillShowUI>
      </DiskConfiguration>
      <ImageInstall><OSImage>
        <InstallFrom><MetaData wcm:action="add"><Key>/IMAGE/NAME</Key><Value>Windows 10 Enterprise Evaluation</Value></MetaData></InstallFrom>
        <InstallTo><DiskID>0</DiskID><PartitionID>1</PartitionID></InstallTo><WillShowUI>OnError</WillShowUI>
      </OSImage></ImageInstall>
      <UserData><AcceptEula>true</AcceptEula><FullName>FotoEditor QA</FullName></UserData>
    </component>
  </settings>
  <settings pass="oobeSystem">
    ${component('Microsoft-Windows-Shell-Setup')}
      <OOBE><HideEULAPage>true</HideEULAPage><HideOnlineAccountScreens>true</HideOnlineAccountScreens><HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE><ProtectYourPC>3</ProtectYourPC></OOBE>
      <UserAccounts><LocalAccounts><LocalAccount wcm:action="add"><Name>FotoEditorQA</Name><Group>Administrators</Group><Password><Value>${password}</Value><PlainText>true</PlainText></Password></LocalAccount></LocalAccounts></UserAccounts>
      <AutoLogon><Enabled>true</Enabled><LogonCount>1</LogonCount><Username>FotoEditorQA</Username><Password><Value>${password}</Value><PlainText>true</PlainText></Password></AutoLogon>
      <FirstLogonCommands><SynchronousCommand wcm:action="add"><Order>1</Order><Description>Verify CashGear FotoEditor</Description>
        <CommandLine>powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "(Get-CimInstance Win32_LogicalDisk | Where-Object VolumeName -eq 'FOTOEDITORQA').DeviceID | ForEach-Object { &amp; (Join-Path $_ 'qa.ps1') }"</CommandLine>
      </SynchronousCommand></FirstLogonCommands>
    </component>
  </settings>
</unattend>
`,
);
await writeFile(
  join(qaMedia, 'qa.ps1'),
  `
$ErrorActionPreference = 'Stop'
$qa = 'C:\\FotoEditorQA'
New-Item -ItemType Directory -Path $qa | Out-Null
try {
  Expand-Archive -LiteralPath (Join-Path $PSScriptRoot 'node.zip') -DestinationPath $qa
  Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'files') -Destination $qa -Recurse
  $node = (Get-ChildItem $qa -Directory -Filter 'node-*-win-x64')[0].FullName + '\\node.exe'
  $env:GPU_TEST_SOFTWARE='1'
  $env:RUNNER_ENVIRONMENT='github-hosted'
  Set-Location (Join-Path $qa 'files')
  $ErrorActionPreference = 'Continue'
  & $node scripts\\desktop-package-smoke.mjs verify assets 2>&1 | Out-File -Encoding utf8 "$qa\\guest.log"
  $exitCode=$LASTEXITCODE
  $ErrorActionPreference = 'Stop'
  if ($exitCode -ne 0) { throw "Installer smoke failed ($exitCode)." }
  Invoke-WebRequest -UseBasicParsing -Uri 'http://10.0.2.2:${port}/guest.log' -Method Put -InFile "$qa\\guest.log" | Out-Null
  Invoke-WebRequest -UseBasicParsing -Uri 'http://10.0.2.2:${port}/verification.json' -Method Put -InFile 'assets\\verification.json' | Out-Null
} catch {
  $_ | Out-String | Add-Content "$qa\\guest.log"
  Invoke-WebRequest -UseBasicParsing -Uri 'http://10.0.2.2:${port}/guest.log' -Method Put -InFile "$qa\\guest.log" | Out-Null
  '{"failed":true}' | Set-Content -Encoding Ascii "$qa\\failed.json"
  Invoke-WebRequest -UseBasicParsing -Uri 'http://10.0.2.2:${port}/verification.json' -Method Put -InFile "$qa\\failed.json" | Out-Null
}
`,
);
await run('genisoimage', ['-quiet', '-J', '-R', '-V', 'FOTOEDITORQA', '-o', qaIso, qaMedia]);
await run('qemu-img', ['create', '-f', 'qcow2', disk, '32G']);
const vm = spawn(
  'qemu-system-x86_64',
  [
    '-enable-kvm',
    '-cpu',
    'host',
    '-smp',
    '3',
    '-m',
    '6144',
    '-display',
    'none',
    '-boot',
    'once=d',
    '-drive',
    'file=' + disk + ',format=qcow2',
    '-cdrom',
    image,
    '-drive',
    'file=' + qaIso + ',media=cdrom,readonly=on',
    '-netdev',
    'user,id=net',
    '-device',
    'e1000,netdev=net',
  ],
  { stdio: 'inherit' },
);
try {
  const deadline = Date.now() + 60 * 60 * 1000;
  while (!result && vm.exitCode === null && Date.now() < deadline)
    await new Promise((resolve) => setTimeout(resolve, 5000));
  for (const name of ['verification.json', 'guest.log'])
    await cp(join(incoming, name), join(output, name)).catch(() => {});
  if (!result || result.failed || result.windows.BuildNumber !== '19045') {
    const log = await readFile(join(output, 'guest.log'), 'utf8').catch(() => '');
    throw new Error('Clean Windows 10 installer verification did not pass.\n' + log);
  }
  console.log(result);
} finally {
  vm.kill();
  server.close();
}
