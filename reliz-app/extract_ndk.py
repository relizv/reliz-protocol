import os
import zipfile
import sys

def extract_zip_with_symlinks(zip_path, extract_dir):
    print(f"Extracting {zip_path} to {extract_dir}...")
    with zipfile.ZipFile(zip_path, 'r') as zip_ref:
        for member in zip_ref.infolist():
            target_path = os.path.join(extract_dir, member.filename)
            
            # Check if it is a directory
            if member.filename.endswith('/') or member.is_dir():
                os.makedirs(target_path, exist_ok=True)
                continue
                
            # High 16 bits of external_attr contain the unix permissions/file type
            # unix S_IFLNK (symlink) is 0o120000 octal = 0xA000 hex
            attr = member.external_attr >> 16
            is_symlink = (attr & 0o170000) == 0o120000
            
            # Ensure parent directory exists
            os.makedirs(os.path.dirname(target_path), exist_ok=True)
            
            # Remove existing file/symlink if any
            if os.path.lexists(target_path):
                try:
                    if os.path.isdir(target_path) and not os.path.islink(target_path):
                        import shutil
                        shutil.rmtree(target_path)
                    else:
                        os.unlink(target_path)
                except Exception as e:
                    print(f"Warning: failed to delete existing path {target_path}: {e}")
            
            if is_symlink:
                # Read the symlink target from the zip content
                link_target = zip_ref.read(member).decode('utf-8')
                try:
                    os.symlink(link_target, target_path)
                except Exception as e:
                    print(f"Failed to create symlink {target_path} -> {link_target}: {e}")
            else:
                # Extract normal file
                zip_ref.extract(member, extract_dir)
                # Restore permissions
                if attr:
                    try:
                        os.chmod(target_path, attr)
                    except Exception as e:
                        pass
    print("Extraction complete!")

if __name__ == '__main__':
    if len(sys.argv) < 3:
        print("Usage: python3 extract_ndk.py <zip_path> <extract_dir>")
        sys.exit(1)
    extract_zip_with_symlinks(sys.argv[1], sys.argv[2])
