#!/bin/bash
HOLE_SZ_MM=1
INCLUDE_MOUNTING_HOLES=true

while read -r fname; do
    file=$(basename "$fname" .kicad_pcb)

    printf "\n\033[32mProcessing\033[0m %s\n" "$file"

    kicad-cli pcb export svg \
        --mode-multi \
        --layers F.Paste,B.Paste \
        --fit-page-to-board \
        --exclude-drawing-sheet \
        --drill-shape-opt 0 \
        --black-and-white \
        -o out "$fname"

    # Export boards with holes to out/holy
    if [[ $INCLUDE_MOUNTING_HOLES ]]; then
        kicad-cli pcb export svg \
            --mode-multi \
            --layers F.Paste,B.Paste \
            --fit-page-to-board \
            --exclude-drawing-sheet \
            --drill-shape-opt 2 \
            --black-and-white \
            -o out/holy "$fname"
    fi

    for side in F_Paste B_Paste; do
        # Remove empty svg's
        if ! (cat "out/$file-$side.svg" | grep "path" > /dev/null); then
            printf "\033[34mRemoving empty file\033[0m out/%s-%s.svg\n" "$file" "$side"
            rm "out/$file-$side.svg"

            if [[ $INCLUDE_MOUNTING_HOLES ]]; then
                rm "out/holy/$file-$side.svg"
            fi
            continue
        fi

        # Merge mounting holes into normal svg (exclude random tiny holes for
        # stuff like connector positioning)
        if [[ $INCLUDE_MOUNTING_HOLES ]]; then
            maybe_cutouts=$( \
                diff "out/$file-$side.svg" "out/holy/$file-$side.svg" | \
                grep circle \
            )

            definite_cutouts=$(echo "$maybe_cutouts" | \
                sed "s/> //" | \
                awk -v hole="$HOLE_SZ_MM" \
                '{ if (gensub(/".*/, "", 1, gensub(/.*r="/, "", 1)) >= hole) print }' \
            )

            if [[ -z "$definite_cutouts" ]]; then
                printf "\033[33mNo valid mounting holes... skipping merge\033[0m\n"
                continue
            fi

            # apply the changes
            sed -i "s/<\/svg>//" "out/$file-$side.svg"
            printf "<g style=\"fill:#000000; fill-opacity:1.0000; stroke:none;\">\n%s\n</g>\n</svg>" \
                "$definite_cutouts" >> "out/$file-$side.svg"

            printf "\033[34mMerging $side,\033[0m total: %s accepted: %s\n" \
                "$(echo "$maybe_cutouts" | wc -l)" \
                "$(echo "$definite_cutouts" | wc -l)"
        fi
    done
done < <(find "$PWD" -type f -name "*.kicad_pcb")

if [[ $INCLUDE_MOUNTING_HOLES ]]; then
    rm -r "out/holy"
fi
