#!/bin/bash

cd public/data

for f in *.flac; do
  ffmpeg -y -i "$f" -c:a flac -sample_fmt s16 "tmp_$f" && mv -f "tmp_$f" "$f"
done
