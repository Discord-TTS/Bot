import os

os.system("xtr src/commands/mod.rs src/error.rs src/funcs.rs -o translations/master.pot")

for language in os.listdir("translations"):
    if os.path.isdir(f"translations/{language}"):
        base = f"translations/{language}/{language}"
        po_file = f"{base}.po"

        print(base, po_file)
        os.system(f"msgmerge --update {po_file} translations/master.pot")
        os.system(f"msgfmt --output-file={base}.mo {po_file}")
