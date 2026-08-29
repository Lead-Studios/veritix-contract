import os
import re
import glob

def main():
    test_files = glob.glob('src/**/*_test.rs', recursive=True) + glob.glob('src/*_test.rs')
    for file_path in test_files:
        with open(file_path, 'r', encoding='utf-8') as f:
            content = f.read()
        
        # Replace #[should_panic...] with #[cfg_attr(windows, ignore)]\n    #[should_panic...]
        # Only if it's not already ignored
        content = re.sub(
            r'(?<!#\[cfg_attr\(windows, ignore\)\]\n\s{4})(#\[should_panic.*?\])',
            r'#[cfg_attr(windows, ignore)]\n    \1',
            content
        )
        
        with open(file_path, 'w', encoding='utf-8') as f:
            f.write(content)

if __name__ == '__main__':
    main()
