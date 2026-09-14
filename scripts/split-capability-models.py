#!/usr/bin/env python3
"""Split qualified capability ZIPs into program archives and shared model files.

The output directory is also a complete offline bundle: keep its models directory
beside the program ZIP. No packages, network access, or signing key are required.
"""
import argparse
import hashlib
import json
import pathlib
import re
import shutil
import tempfile
import urllib.parse
import zipfile

MODEL_SUFFIXES = {'.onnx', '.bin', '.txt', '.json', '.safetensors', '.md'}


def digest(path):
    value = hashlib.sha256()
    with open(path, 'rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def safe_path(name):
    parts = name.rstrip('/').split('/')
    return bool(parts) and all(part not in ('', '.', '..') for part in parts) and '\\' not in name and ':' not in name


def https_base(value):
    url = urllib.parse.urlparse(value)
    if url.scheme != 'https' or not url.netloc or url.username or url.password or url.query or url.fragment:
        raise ValueError('distribution base must be an HTTPS URL without credentials, query or fragment')
    return value.rstrip('/') + '/'


def split_entry(entry, archive, output, base_url, model_base_url):
    if digest(archive) != entry['archiveSha256']:
        raise ValueError('source archive checksum does not match catalog')
    result = dict(entry)
    models = []
    archive_name = pathlib.Path(urllib.parse.unquote(urllib.parse.urlparse(entry['url']).path)).name
    if not archive_name.endswith('.zip') or not safe_path(archive_name):
        raise ValueError('invalid program archive name')
    target = output / archive_name
    if target.exists():
        raise ValueError('output program archive already exists')
    with zipfile.ZipFile(archive) as source:
        infos = source.infolist()
        if any(not safe_path(item.filename) or (item.external_attr >> 16) & 0o170000 == 0o120000 for item in infos):
            raise ValueError('source archive contains unsafe paths or symlinks')
        # Linux payloads may contain distinct Foo.py/foo.py files. Use the
        # target's filesystem rules, not the host running this archive rewrite.
        case_sensitive = entry['targetTriple'] == 'x86_64-unknown-linux-gnu'
        seen = {}
        for item in infos:
            name = item.filename.rstrip('/')
            key = name if case_sensitive else name.casefold()
            if key in seen:
                raise ValueError(f'source archive has duplicate paths: {seen[key]} / {item.filename}')
            seen[key] = item.filename
        manifest = json.loads(source.read('manifest.json'))
        inventory = {item['path']: item for item in manifest['files']}
        if manifest['packId'] != entry['capabilityId'] or manifest['version'] != entry['version']:
            raise ValueError('manifest identity does not match catalog')
        model_names = {item.filename for item in infos if not item.is_dir() and item.filename.startswith('models/')}
        # Model objects are shared across platforms; their catalog paths must
        # remain unambiguous even when the program targets Linux.
        if len({name.casefold() for name in model_names}) != len(model_names):
            raise ValueError('model files have case-insensitive duplicate paths')
        if len(model_names) > 1024:
            raise ValueError('model file count exceeds 1024')
        for name in model_names:
            if pathlib.PurePosixPath(name).suffix not in MODEL_SUFFIXES:
                raise ValueError(f'unsupported model data format: {name}')
            if name not in inventory:
                raise ValueError(f'model missing from source inventory: {name}')
            declaration = inventory[name]
            if not re.fullmatch(r'[0-9a-f]{64}', declaration.get('sha256', '')) or set(declaration['sha256']) == {'0'}:
                raise ValueError('model SHA-256 is invalid')
            if type(declaration.get('bytes')) is not int or not 0 < declaration['bytes'] <= 8 * 1024 ** 3:
                raise ValueError('model byte size is invalid')
        if sum(inventory[name]['bytes'] for name in model_names) > 16 * 1024 ** 3:
            raise ValueError('models exceed 16 GiB')
        manifest.update(schemaVersion=3, signingKeyId='', signature='')
        manifest_bytes = (json.dumps(manifest, ensure_ascii=False, indent=2) + '\n').encode()
        with tempfile.NamedTemporaryFile(dir=output, suffix='.zip', delete=False) as temporary:
            temporary_path = pathlib.Path(temporary.name)
        try:
            with zipfile.ZipFile(temporary_path, 'w', compression=zipfile.ZIP_DEFLATED) as destination:
                for info in infos:
                    if info.filename == 'manifest.json':
                        destination.writestr(info, manifest_bytes)
                    elif info.filename in model_names:
                        declaration = inventory[info.filename]
                        relative = pathlib.PurePosixPath('models') / declaration['sha256'] / pathlib.PurePosixPath(info.filename).name
                        model_path = output.joinpath(*relative.parts)
                        model_path.parent.mkdir(parents=True, exist_ok=True)
                        with tempfile.NamedTemporaryFile(dir=model_path.parent, delete=False) as model_temp:
                            model_temp_path = pathlib.Path(model_temp.name)
                            with source.open(info) as data:
                                shutil.copyfileobj(data, model_temp, length=1024 * 1024)
                        try:
                            if model_temp_path.stat().st_size != declaration['bytes'] or digest(model_temp_path) != declaration['sha256']:
                                raise ValueError(f'model inventory mismatch: {info.filename}')
                            if model_path.exists():
                                if digest(model_path) != declaration['sha256']:
                                    raise ValueError('existing model object is invalid')
                            else:
                                model_temp_path.replace(model_path)
                        finally:
                            model_temp_path.unlink(missing_ok=True)
                        # The models/ prefix is included in the distribution URL and
                        # offline layout so a single static directory serves both.
                        models.append(dict(path=info.filename, sha256=declaration['sha256'], bytes=declaration['bytes'],
                                           urls=[urllib.parse.urljoin(model_base_url, urllib.parse.quote(str(relative))) ]))
                    else:
                        with source.open(info) as data, destination.open(info, 'w') as member:
                            shutil.copyfileobj(data, member, length=1024 * 1024)
            temporary_path.replace(target)
        finally:
            temporary_path.unlink(missing_ok=True)
    with zipfile.ZipFile(target) as emitted:
        installed_bytes = sum(info.file_size for info in emitted.infolist() if not info.is_dir())
    result.update(url=urllib.parse.urljoin(base_url, urllib.parse.quote(archive_name)),
                  archiveSha256=digest(target), compressedBytes=target.stat().st_size, installedBytes=installed_bytes,
                  manifestSha256=hashlib.sha256(manifest_bytes).hexdigest(), signingKeyId='',
                  modelFiles=sorted(models, key=lambda item: item['path']), modelBytes=sum(item['bytes'] for item in models) if models else None)
    result.pop('archiveChunks', None)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--catalog', required=True, type=pathlib.Path)
    parser.add_argument('--archives', required=True, type=pathlib.Path)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    parser.add_argument('--base-url', required=True)
    parser.add_argument('--model-base-url')
    args = parser.parse_args()
    base = https_base(args.base_url)
    model_base = https_base(args.model_base_url or args.base_url)
    catalog = json.loads(args.catalog.read_text())
    if args.output.exists():
        raise ValueError('output must be a new directory')
    args.output.mkdir(parents=True)
    entries = []
    for entry in catalog['entries']:
        archive = args.archives / pathlib.Path(urllib.parse.unquote(urllib.parse.urlparse(entry['url']).path)).name
        entries.append(split_entry(entry, archive, args.output, base, model_base))
    (args.output / 'install-catalog.json').write_text(json.dumps(dict(schemaVersion=1, entries=entries), ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(dict(entries=len(entries), modelFiles=sum(len(entry['modelFiles']) for entry in entries))))


if __name__ == '__main__':
    main()
