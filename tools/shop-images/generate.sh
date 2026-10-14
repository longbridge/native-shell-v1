#!/usr/bin/env bash
# Generates the product illustrations used by examples/shop.
#
# Each product is drawn once as an SVG group in a 600 x 600 frame; detail
# shots reuse the same drawing under a zoom transform so the close-ups match
# the main shots exactly. SVGs are written to tools/shop-images/svg/, PNGs to
# examples/shop/images/.
#
# Requires rsvg-convert and ImageMagick (magick).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
svg_dir="$here/svg"
out_dir="$root/examples/shop/images"
work_dir="$root/target/shop-images"
budget=40000
mkdir -p "$svg_dir" "$out_dir" "$work_dir"

# Deterministic speckles: count x y w h color opacity seed max-radius.
speckles() {
  awk -v n="$1" -v x="$2" -v y="$3" -v w="$4" -v h="$5" -v c="$6" \
      -v o="$7" -v seed="$8" -v r="$9" 'BEGIN {
    srand(seed)
    for (i = 0; i < n; i++) {
      printf "<circle cx=\"%.1f\" cy=\"%.1f\" r=\"%.2f\" fill=\"%s\" opacity=\"%.2f\"/>\n",
        x + rand() * w, y + rand() * h, 0.5 + rand() * r, c, o * (0.5 + rand() * 0.5)
    }
  }'
}

# Soft contact shadow: cx cy rx ry.
shadow() {
  cat <<EOF
<ellipse cx="$1" cy="$2" rx="$3" ry="$4" fill="#3a2f22" opacity="0.16" filter="url(#blur)"/>
<ellipse cx="$1" cy="$(($2 - 2))" rx="$(($3 * 8 / 10))" ry="$(($4 / 2))" fill="#3a2f22" opacity="0.22" filter="url(#blur-s)"/>
EOF
}

# ---------------------------------------------------------------- products

mug() { # base light dark
  local B=$1 L=$2 D=$3
  shadow 282 466 160 20
  cat <<EOF
<defs>
<linearGradient id="glaze" x1="140" x2="400" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$D"/><stop offset="0.3" stop-color="$L"/>
<stop offset="0.6" stop-color="$B"/><stop offset="1" stop-color="$D"/></linearGradient>
<linearGradient id="handle" x1="0" y1="210" x2="0" y2="400" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$L"/><stop offset="1" stop-color="$D"/></linearGradient>
<linearGradient id="clay" x1="140" x2="400" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="#a89379"/><stop offset="0.3" stop-color="#ddcdb3"/>
<stop offset="1" stop-color="#a08a6e"/></linearGradient>
<linearGradient id="inside" x1="0" y1="138" x2="0" y2="186" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$D"/><stop offset="1" stop-color="$B"/></linearGradient>
<clipPath id="mug-body"><path d="M140 160 L150 448 A120 26 0 0 0 390 448 L400 160 Z"/></clipPath>
</defs>
<path d="M394 222 C 500 210, 508 396, 390 386" fill="none" stroke="url(#handle)" stroke-width="30" stroke-linecap="round"/>
<path d="M412 226 C 482 226, 486 360, 420 372" fill="none" stroke="#ffffff" stroke-opacity="0.22" stroke-width="5" stroke-linecap="round"/>
<path d="M140 160 L150 448 A120 26 0 0 0 390 448 L400 160 Z" fill="url(#glaze)"/>
<g clip-path="url(#mug-body)">
<path d="M130 428 C 170 440, 200 446, 236 452 C 260 456, 268 450, 290 452 C 330 456, 370 446, 410 430 L410 520 L130 520 Z" fill="url(#clay)"/>
<path d="M130 428 C 170 440, 200 446, 236 452 C 260 456, 268 450, 290 452 C 330 456, 370 446, 410 430" fill="none" stroke="$D" stroke-opacity="0.45" stroke-width="3"/>
$(speckles 120 140 170 260 270 "$D" 0.55 7 1.4)
$(speckles 60 140 170 260 270 "#ffffff" 0.35 11 1.1)
<rect x="182" y="172" width="30" height="262" rx="15" fill="#ffffff" opacity="0.22" filter="url(#blur-s)"/>
</g>
<ellipse cx="270" cy="160" rx="130" ry="28" fill="$L"/>
<ellipse cx="270" cy="162" rx="119" ry="23" fill="url(#inside)"/>
EOF
}

kettle() { # base light dark
  local B=$1 L=$2 D=$3
  shadow 300 474 170 18
  cat <<EOF
<defs>
<linearGradient id="body" x1="196" x2="414" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$D"/><stop offset="0.28" stop-color="$L"/>
<stop offset="0.55" stop-color="$B"/><stop offset="1" stop-color="$D"/></linearGradient>
<linearGradient id="lid" x1="227" x2="383" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$D"/><stop offset="0.35" stop-color="$L"/><stop offset="1" stop-color="$D"/></linearGradient>
<linearGradient id="metal" x1="80" y1="220" x2="220" y2="460" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$B"/><stop offset="1" stop-color="$D"/></linearGradient>
</defs>
<path d="M398 314 C 470 298, 482 318, 478 360 L470 420 C 467 440, 442 446, 412 440" fill="none" stroke="$D" stroke-width="22" stroke-linecap="round" stroke-linejoin="round"/>
<path d="M404 312 C 462 302, 472 318, 470 352" fill="none" stroke="$L" stroke-opacity="0.6" stroke-width="5" stroke-linecap="round"/>
<path d="M226 442 C 178 446, 158 432, 154 400" fill="none" stroke="url(#metal)" stroke-width="26" stroke-linecap="round"/>
<path d="M214 446 C 150 450, 140 420, 145 370 C 150 310, 142 262, 96 238 C 90 235, 84 236, 80 241" fill="none" stroke="url(#metal)" stroke-width="13" stroke-linecap="round" stroke-linejoin="round"/>
<path d="M146 400 C 142 380, 146 340, 146 312" fill="none" stroke="#ffffff" stroke-opacity="0.35" stroke-width="3" stroke-linecap="round"/>
<path d="M210 290 L196 452 Q195 474 218 474 L392 474 Q415 474 414 452 L400 290 Z" fill="url(#body)"/>
<rect x="232" y="300" width="22" height="160" rx="11" fill="#ffffff" opacity="0.18" filter="url(#blur-s)"/>
<path d="M197 444 L413 444" stroke="$D" stroke-opacity="0.5" stroke-width="2"/>
<ellipse cx="305" cy="290" rx="95" ry="15" fill="$L"/>
<path d="M226 290 C 238 254, 372 254, 384 290 A79 12 0 0 1 226 290 Z" fill="url(#lid)"/>
<rect x="298" y="246" width="14" height="14" fill="$D"/>
<ellipse cx="305" cy="246" rx="20" ry="9" fill="$D"/>
<ellipse cx="301" cy="244" rx="9" ry="3" fill="$L" opacity="0.6"/>
EOF
}

towels() { # set of two, natural flax with a thin stripe
  local B="#ddd0b8" L="#ece3d2" D="#b7a585" S="#a64a3a"
  shadow 300 448 190 16
  cat <<EOF
<defs>
<pattern id="weave" width="6" height="6" patternUnits="userSpaceOnUse">
<path d="M0 1 H3 M3 4 H6 M4.5 0 V3 M1.5 3 V6" stroke="#5a4a30" stroke-opacity="0.11" stroke-width="1.1"/></pattern>
<linearGradient id="top" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="$B"/><stop offset="1" stop-color="$L"/></linearGradient>
<linearGradient id="front" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="$L"/><stop offset="1" stop-color="$D"/></linearGradient>
</defs>
<!-- bottom towel -->
<path d="M152 304 L448 304 Q458 304 462 314 L482 392 L118 392 L138 314 Q142 304 152 304 Z" fill="url(#top)"/>
<path d="M118 392 L482 392 L482 424 Q482 444 462 444 L138 444 Q118 444 118 424 Z" fill="url(#front)"/>
<path d="M120 408 H480 M120 426 H480" stroke="$D" stroke-opacity="0.55" stroke-width="1.5"/>
<path d="M176 304 L160 392 L160 444 M186 304 L171 392 L171 444 M414 304 L430 392 L430 444 M424 304 L441 392 L441 444" fill="none" stroke="$S" stroke-width="3"/>
<!-- top towel -->
<path d="M166 222 L434 222 Q444 222 448 232 L468 318 L132 318 L152 232 Q156 222 166 222 Z" fill="url(#top)"/>
<path d="M132 318 L468 318 L468 346 Q468 364 450 364 L150 364 Q132 364 132 346 Z" fill="url(#front)"/>
<path d="M134 332 H466 M134 348 H466" stroke="$D" stroke-opacity="0.55" stroke-width="1.5"/>
<path d="M190 222 L173 318 L173 364 M200 222 L184 318 L184 364 M400 222 L417 318 L417 364 M410 222 L428 318 L428 364" fill="none" stroke="$S" stroke-width="3"/>
<path d="M166 222 L434 222 Q444 222 448 232 L468 318 L468 346 Q468 364 450 364 L150 364 Q132 364 132 346 L132 318 L152 232 Q156 222 166 222 Z" fill="url(#weave)"/>
<path d="M152 304 L448 304 L482 392 L482 424 Q482 444 462 444 L138 444 Q118 444 118 424 L118 392 Z" fill="url(#weave)" opacity="0.8"/>
$(speckles 30 160 230 280 80 "$D" 0.6 3 1.2)
<path d="M222 226 C 210 170, 266 166, 254 226" fill="none" stroke="$D" stroke-width="8" stroke-linecap="round"/>
<path d="M222 226 C 210 170, 266 166, 254 226" fill="none" stroke="$L" stroke-opacity="0.6" stroke-width="3" stroke-linecap="round"/>
<rect x="212" y="220" width="52" height="10" rx="3" fill="$D" opacity="0.5"/>
EOF
}

lamp() { # base light dark glow(0|1)
  local B=$1 L=$2 D=$3 G=${4:-0}
  local s0="#cdbfa6" s1="#f6efe3" s2="#ece2d0" s3="#cbbda3" inner="#e6dbc7" rim="#c4b497"
  if [ "$G" = 1 ]; then
    s0="#eac68a" s1="#fff4da" s2="#f9e2b4" s3="#e2b878" inner="#fffaec" rim="#efc983"
  fi
  shadow 300 500 120 14
  cat <<EOF
<defs>
<linearGradient id="shade" x1="160" x2="440" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$s0"/><stop offset="0.32" stop-color="$s1"/>
<stop offset="0.65" stop-color="$s2"/><stop offset="1" stop-color="$s3"/></linearGradient>
<radialGradient id="base" cx="0.38" cy="0.4" r="0.7">
<stop offset="0" stop-color="$L"/><stop offset="0.6" stop-color="$B"/><stop offset="1" stop-color="$D"/></radialGradient>
<radialGradient id="halo" cx="0.5" cy="0.5" r="0.5">
<stop offset="0" stop-color="#ffd890" stop-opacity="0.55"/><stop offset="1" stop-color="#ffd890" stop-opacity="0"/></radialGradient>
<radialGradient id="opening" cx="0.5" cy="0.35" r="0.6">
<stop offset="0" stop-color="$inner"/><stop offset="1" stop-color="$rim"/></radialGradient>
<pattern id="linen" width="4" height="4" patternUnits="userSpaceOnUse">
<path d="M0 0.5 H4 M0.5 0 V4" stroke="#6b5a3c" stroke-opacity="0.045" stroke-width="1"/></pattern>
</defs>
EOF
  [ "$G" = 1 ] && echo '<ellipse cx="300" cy="200" rx="230" ry="180" fill="url(#halo)"/>'
  cat <<EOF
<rect x="292" y="282" width="16" height="44" fill="#b89a64"/>
<rect x="292" y="282" width="5" height="44" fill="#d9c08c"/>
<path d="M278 322 C 278 336, 215 346, 205 410 C 198 462, 240 500, 300 500 C 360 500, 402 462, 395 410 C 385 346, 322 336, 322 322 Z" fill="url(#base)"/>
<ellipse cx="300" cy="322" rx="22" ry="5" fill="$D" opacity="0.6"/>
<ellipse cx="300" cy="120" rx="110" ry="18" fill="url(#opening)"/>
<path d="M190 120 L160 285 A140 22 0 0 0 440 285 L410 120 A110 18 0 0 1 190 120 Z" fill="url(#shade)"/>
<path d="M190 120 L160 285 A140 22 0 0 0 440 285 L410 120 A110 18 0 0 1 190 120 Z" fill="url(#linen)"/>
<path d="M161 279 A140 22 0 0 0 439 279" fill="none" stroke="$s3" stroke-width="5"/>
<path d="M191 125 A110 18 0 0 0 409 125" fill="none" stroke="$s3" stroke-width="4"/>
EOF
  [ "$G" = 1 ] && echo '<path d="M164 287 A137 20 0 0 0 436 287" fill="none" stroke="#fff6dc" stroke-width="3" opacity="0.9"/>'
  return 0
}

throw_defs() { # base light dark
  local B=$1 L=$2 D=$3
  cat <<EOF
<pattern id="herring" width="24" height="16" patternUnits="userSpaceOnUse">
<path d="M0 8 L11 0 M0 16 L11 8 M0 24 L11 16 M13 -4 L24 4 M13 4 L24 12 M13 12 L24 20" stroke="$D" stroke-opacity="0.5" stroke-width="2.6"/>
<path d="M12 0 V16" stroke="$D" stroke-opacity="0.25" stroke-width="1.2"/></pattern>
<linearGradient id="top" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="$B"/><stop offset="1" stop-color="$L"/></linearGradient>
<linearGradient id="layer" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="$L"/><stop offset="0.55" stop-color="$B"/><stop offset="1" stop-color="$D"/></linearGradient>
EOF
}

throw() { # base light dark
  local B=$1 L=$2 D=$3
  shadow 300 446 200 16
  echo '<defs>'
  throw_defs "$B" "$L" "$D"
  echo '</defs>'
  cat <<EOF
<g stroke-linecap="round" fill="none">
$(awk -v b="$B" -v d="$D" 'BEGIN { for (y = 238; y < 440; y += 8) { droop = 6 + (y % 3) * 2;
  printf "<path d=\"M456 %d q18 1 30 %d\" stroke=\"%s\" stroke-width=\"4\"/>\n", y, droop, b;
  printf "<path d=\"M462 %d q14 1 24 %d\" stroke=\"%s\" stroke-opacity=\"0.4\" stroke-width=\"1.2\"/>\n", y + 1.5, droop, d } }')
</g>
<path d="M150 228 L450 228 Q460 228 462 238 L476 320 L124 320 L138 238 Q140 228 150 228 Z" fill="url(#top)"/>
<path d="M150 228 L450 228 Q460 228 462 238 L476 320 L124 320 L138 238 Q140 228 150 228 Z" fill="url(#herring)"/>
<path d="M124 320 H476 V352 Q476 362 466 362 H134 Q124 362 124 352 Z" fill="url(#layer)"/>
<path d="M120 362 H480 V394 Q480 404 470 404 H130 Q120 404 120 394 Z" fill="url(#layer)"/>
<path d="M118 404 H482 V434 Q482 446 470 446 H130 Q118 446 118 434 Z" fill="url(#layer)"/>
<path d="M124 320 H476 V352 Q476 362 466 362 H134 Q124 362 124 352 Z M120 362 H480 V394 Q480 404 470 404 H130 Q120 404 120 394 Z M118 404 H482 V434 Q482 446 470 446 H130 Q118 446 118 434 Z" fill="url(#herring)" opacity="0.6"/>
EOF
}

throw_detail() { # close-up of the weave and the fringed end
  local B=$1 L=$2 D=$3
  echo '<defs>'
  cat <<EOF
<ellipse cx="300" cy="500" rx="300" ry="40" fill="#3a2f22" opacity="0.1" filter="url(#blur)"/>
EOF
  throw_defs "$B" "$L" "$D"
  cat <<EOF
<linearGradient id="panel" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="$B"/><stop offset="0.75" stop-color="$L"/><stop offset="1" stop-color="$B"/></linearGradient>
<clipPath id="panel-clip"><path d="M-10 -10 H610 V372 C 520 384, 420 368, 300 378 C 180 388, 80 370, -10 380 Z"/></clipPath>
</defs>
<g stroke-linecap="round" fill="none">
$(awk -v b="$B" -v d="$D" -v l="$L" 'BEGIN { srand(4); for (x = 20; x <= 590; x += 26) { s = rand() * 10 - 5; y0 = 372; len = 120 + rand() * 18;
  printf "<path d=\"M%.1f %d q%.1f %.1f %.1f %.1f\" stroke=\"%s\" stroke-width=\"15\"/>\n", x, y0, s, len / 2, s * 1.6, len, d;
  printf "<path d=\"M%.1f %d q%.1f %.1f %.1f %.1f\" stroke=\"%s\" stroke-width=\"12\"/>\n", x, y0, s, len / 2, s * 1.6, len, b;
  printf "<path d=\"M%.1f %d q%.1f %.1f %.1f %.1f\" stroke=\"%s\" stroke-opacity=\"0.8\" stroke-width=\"3\"/>\n", x - 3, y0, s, len / 2, s * 1.6, len - 10, l;
  for (k = 392; k < y0 + len - 16; k += 13) { t = (k - y0) / len; px = x + s * 2 * t * (1 - t) + s * 1.6 * t * t;
    printf "<path d=\"M%.1f %.1f l9 -6\" stroke=\"%s\" stroke-opacity=\"0.45\" stroke-width=\"2\"/>\n", px - 4.5, k + 3, d } } }')
</g>
<g clip-path="url(#panel-clip)">
<rect width="600" height="400" fill="url(#panel)"/>
<rect width="250" height="170" fill="url(#herring)" transform="scale(2.4)" opacity="0.75"/>
<rect y="340" width="600" height="60" fill="$D" opacity="0.18"/>
</g>
<path d="M-10 346 C 80 336, 180 354, 300 344 C 420 334, 520 350, 610 338" fill="none" stroke="$D" stroke-opacity="0.4" stroke-width="2" stroke-dasharray="7 5"/>
EOF
}

planter() {
  local B="#c8754e" L="#e39d74" D="#93502f"
  shadow 300 492 150 16
  cat <<EOF
<defs>
<linearGradient id="clay" x1="190" x2="410" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$D"/><stop offset="0.3" stop-color="$L"/>
<stop offset="0.6" stop-color="$B"/><stop offset="1" stop-color="$D"/></linearGradient>
<linearGradient id="saucer" x1="172" x2="428" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$D"/><stop offset="0.35" stop-color="$L"/><stop offset="1" stop-color="$D"/></linearGradient>
<linearGradient id="leaf" x1="0" y1="0" x2="1" y2="1">
<stop offset="0" stop-color="#7aa565"/><stop offset="1" stop-color="#3f6e3c"/></linearGradient>
<linearGradient id="leaf2" x1="1" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="#8bb574"/><stop offset="1" stop-color="#4a7b45"/></linearGradient>
<clipPath id="pot"><path d="M210 336 L232 470 Q234 478 244 478 L356 478 Q366 478 368 470 L390 336 Z"/></clipPath>
</defs>
<path d="M174 474 L180 488 Q184 496 196 496 L404 496 Q416 496 420 488 L426 474 Z" fill="url(#saucer)"/>
<ellipse cx="300" cy="474" rx="126" ry="14" fill="$L"/>
<ellipse cx="300" cy="474" rx="112" ry="10" fill="$D" opacity="0.55"/>
<g stroke="#4f7d43" stroke-width="4" stroke-linecap="round" fill="none">
<path d="M300 300 C 296 250, 270 220, 236 196"/><path d="M302 300 C 306 240, 330 200, 360 168"/>
<path d="M298 300 C 290 270, 250 266, 214 262"/><path d="M304 300 C 316 272, 350 262, 388 254"/>
<path d="M300 300 C 300 240, 300 180, 302 120"/>
</g>
EOF
  # leaf: x y rotation length width gradient
  local leaves="236 196 -48 96 64 leaf
360 168 40 104 66 leaf2
214 262 -78 92 60 leaf2
388 254 76 92 60 leaf
302 120 2 96 62 leaf
268 226 -20 84 54 leaf2
336 214 22 84 54 leaf"
  while read -r x y r l w g; do
    cat <<EOF
<g transform="translate($x $y) rotate($r)">
<path d="M0 0 C $((w * 6 / 10)) -$((l / 5)), $((w * 6 / 10)) -$((l * 3 / 4)), 0 -$l C -$((w * 6 / 10)) -$((l * 3 / 4)), -$((w * 6 / 10)) -$((l / 5)), 0 0 Z" fill="url(#$g)"/>
<path d="M0 -4 L0 -$((l - 10))" stroke="#2f5a2f" stroke-opacity="0.45" stroke-width="2" stroke-linecap="round"/>
</g>
EOF
  done <<<"$leaves"
  cat <<EOF
<path d="M210 336 L232 470 Q234 478 244 478 L356 478 Q366 478 368 470 L390 336 Z" fill="url(#clay)"/>
<g clip-path="url(#pot)">
$(speckles 90 205 336 190 145 "$D" 0.6 21 1.4)
$(speckles 50 205 336 190 145 "#f4c7a6" 0.5 22 1.1)
<rect x="250" y="340" width="20" height="132" rx="10" fill="#ffffff" opacity="0.16" filter="url(#blur-s)"/>
</g>
<path d="M196 302 L404 302 L400 340 L200 340 Z" fill="url(#clay)"/>
<path d="M200 340 L400 340" stroke="$D" stroke-width="3" opacity="0.6"/>
<ellipse cx="300" cy="302" rx="104" ry="16" fill="$L"/>
<ellipse cx="300" cy="303" rx="92" ry="12" fill="#5a3d2a"/>
<ellipse cx="300" cy="305" rx="86" ry="9" fill="#6e4c34"/>
$(speckles 20 198 304 200 34 "$D" 0.5 23 1.2)
EOF
}

daypack() { # base light dark
  local B=$1 L=$2 D=$3
  local LB="#8b5a36" LL="#ad7a4f" LD="#5f3a20" BR="#d1ab58"
  shadow 300 500 150 16
  cat <<EOF
<defs>
<linearGradient id="canvas" x1="180" x2="420" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$D"/><stop offset="0.3" stop-color="$L"/>
<stop offset="0.65" stop-color="$B"/><stop offset="1" stop-color="$D"/></linearGradient>
<linearGradient id="flap" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="$L"/><stop offset="1" stop-color="$B"/></linearGradient>
<linearGradient id="leather" x1="0" x2="1">
<stop offset="0" stop-color="$LL"/><stop offset="1" stop-color="$LB"/></linearGradient>
<pattern id="weave" width="4" height="4" patternUnits="userSpaceOnUse">
<path d="M0 0.5 H4 M0.5 0 V4" stroke="#000000" stroke-opacity="0.04" stroke-width="1"/></pattern>
</defs>
<path d="M190 210 C 160 260, 160 420, 176 480" fill="none" stroke="$D" stroke-width="20" stroke-linecap="round"/>
<path d="M410 210 C 440 260, 440 420, 424 480" fill="none" stroke="$D" stroke-width="20" stroke-linecap="round"/>
<path d="M270 156 C 270 108, 330 108, 330 156" fill="none" stroke="$LB" stroke-width="12" stroke-linecap="round"/>
<path d="M273 150 C 276 118, 324 118, 327 150" fill="none" stroke="$LL" stroke-width="3" stroke-linecap="round" opacity="0.7"/>
<rect x="180" y="146" width="240" height="354" rx="64" fill="url(#canvas)"/>
<rect x="180" y="146" width="240" height="354" rx="64" fill="url(#weave)"/>
<path d="M180 210 C 180 166, 210 146, 250 146 L350 146 C 390 146, 420 166, 420 210 L420 280 Q420 302 398 302 L202 302 Q180 302 180 280 Z" fill="url(#flap)"/>
<path d="M192 210 C 192 174, 216 158, 252 158 L348 158 C 384 158, 408 174, 408 210 L408 278 Q408 290 396 290 L204 290 Q192 290 192 278 Z" fill="none" stroke="$D" stroke-opacity="0.55" stroke-width="1.6" stroke-dasharray="5 4"/>
<path d="M180 302 H420" stroke="$D" stroke-opacity="0.5" stroke-width="2"/>
<rect x="212" y="336" width="176" height="140" rx="26" fill="$B"/>
<rect x="212" y="336" width="176" height="140" rx="26" fill="url(#weave)"/>
<rect x="221" y="345" width="158" height="122" rx="20" fill="none" stroke="$D" stroke-opacity="0.55" stroke-width="1.6" stroke-dasharray="5 4"/>
<path d="M212 362 H388" stroke="$D" stroke-opacity="0.35" stroke-width="2"/>
<rect x="196" y="190" width="200" height="40" rx="18" fill="#ffffff" opacity="0.12" filter="url(#blur-s)"/>
EOF
  for x in 242 340; do
    cat <<EOF
<rect x="$x" y="236" width="18" height="170" rx="4" fill="url(#leather)"/>
<path d="M$((x + 3)) 240 V398 M$((x + 15)) 240 V398" stroke="$LD" stroke-opacity="0.6" stroke-width="1" stroke-dasharray="3 3"/>
<rect x="$((x - 1))" y="404" width="20" height="40" rx="3" fill="$LB"/>
<rect x="$((x - 4))" y="372" width="26" height="22" rx="4" fill="none" stroke="$BR" stroke-width="4"/>
<path d="M$((x + 9)) 372 V394" stroke="$BR" stroke-width="3"/>
<circle cx="$((x + 9))" cy="347" r="2.6" fill="$LD"/>
<circle cx="$((x + 9))" cy="330" r="2.6" fill="$LD"/>
EOF
  done
}

tote() {
  local B="#d4b98e" L="#e9d6b2" D="#a88b5f" LB="#8a5632" LL="#b07a4c" LD="#5e3519"
  shadow 300 500 175 16
  cat <<EOF
<defs>
<linearGradient id="canvas" x1="140" x2="460" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$D"/><stop offset="0.3" stop-color="$L"/>
<stop offset="0.65" stop-color="$B"/><stop offset="1" stop-color="$D"/></linearGradient>
<pattern id="weave" width="4" height="4" patternUnits="userSpaceOnUse">
<path d="M0 0.5 H4 M0.5 0 V4" stroke="#5a4528" stroke-opacity="0.055" stroke-width="1"/></pattern>
</defs>
<path d="M236 262 C 236 140, 364 140, 364 262" fill="none" stroke="$LD" stroke-width="16" stroke-linecap="round"/>
<path d="M156 250 L444 250 L460 478 Q462 500 440 500 L160 500 Q138 500 140 478 Z" fill="url(#canvas)"/>
<path d="M156 250 L444 250 L460 478 Q462 500 440 500 L160 500 Q138 500 140 478 Z" fill="url(#weave)"/>
<path d="M156 250 L444 250 L446 282 L154 282 Z" fill="$D" opacity="0.35"/>
<path d="M155 274 H445" stroke="$D" stroke-width="1.6" stroke-dasharray="5 4"/>
<path d="M156 250 L444 250" stroke="$L" stroke-width="3"/>
<path d="M200 300 L196 488 M404 300 L408 488" stroke="$D" stroke-opacity="0.25" stroke-width="2"/>
<rect x="196" y="294" width="40" height="190" rx="20" fill="#ffffff" opacity="0.14" filter="url(#blur-s)"/>
<path d="M220 300 C 214 112, 386 112, 380 300" fill="none" stroke="$LB" stroke-width="18" stroke-linecap="round"/>
<path d="M224 270 C 220 132, 380 132, 376 270" fill="none" stroke="$LL" stroke-width="4" stroke-linecap="round" opacity="0.7"/>
EOF
  for x in 220 380; do
    cat <<EOF
<rect x="$((x - 18))" y="258" width="36" height="58" rx="5" fill="$LB"/>
<rect x="$((x - 13))" y="263" width="26" height="48" rx="3" fill="none" stroke="#f1dfc0" stroke-opacity="0.85" stroke-width="1.4" stroke-dasharray="4 3"/>
<circle cx="$x" cy="276" r="4.5" fill="#c8a256"/><circle cx="$((x - 1))" cy="275" r="1.6" fill="#f3dc96"/>
<circle cx="$x" cy="298" r="4.5" fill="#c8a256"/><circle cx="$((x - 1))" cy="297" r="1.6" fill="#f3dc96"/>
EOF
  done
}

notebook() {
  local B="#2f5b45" L="#477a5d" D="#1f3f2f"
  shadow 304 494 140 14
  cat <<EOF
<defs>
<linearGradient id="cover" x1="176" x2="424" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$D"/><stop offset="0.08" stop-color="$L"/>
<stop offset="0.5" stop-color="$B"/><stop offset="1" stop-color="$D"/></linearGradient>
<pattern id="cloth" width="3" height="3" patternUnits="userSpaceOnUse">
<path d="M0 0.5 H3 M0.5 0 V3" stroke="#000000" stroke-opacity="0.1" stroke-width="0.8"/></pattern>
</defs>
<g transform="rotate(-3 300 300)">
<path d="M262 486 L262 532 L270 524 L278 532 L278 486 Z" fill="#b5483c"/>
<rect x="182" y="112" width="248" height="380" rx="10" fill="#e9e1cd"/>
<path d="M426 122 V482" stroke="#c9bfa6" stroke-width="1"/>
<path d="M190 488 H420" stroke="#c9bfa6" stroke-width="1"/>
<rect x="176" y="106" width="248" height="380" rx="10" fill="url(#cover)"/>
<rect x="176" y="106" width="248" height="380" rx="10" fill="url(#cloth)"/>
<path d="M198 108 V484" stroke="$D" stroke-opacity="0.7" stroke-width="2"/>
<path d="M200 108 V484" stroke="$L" stroke-opacity="0.5" stroke-width="1"/>
<rect x="388" y="102" width="12" height="390" rx="2" fill="#1a221d"/>
<rect x="390" y="102" width="3" height="390" fill="#3b4740"/>
<rect x="230" y="130" width="40" height="330" rx="20" fill="#ffffff" opacity="0.07" filter="url(#blur-s)"/>
</g>
EOF
}

notebook_open() {
  local B="#2f5b45" D="#1f3f2f" P="#fbf8f0" PD="#ebe5d6"
  shadow 300 452 230 16
  cat <<EOF
<defs>
<pattern id="dots" x="0" y="0" width="16" height="16" patternUnits="userSpaceOnUse">
<circle cx="8" cy="8" r="1.4" fill="#a49d8c"/></pattern>
<linearGradient id="left" x1="0" x2="1">
<stop offset="0" stop-color="$PD"/><stop offset="0.12" stop-color="$P"/>
<stop offset="0.85" stop-color="$P"/><stop offset="1" stop-color="#d8d0bc"/></linearGradient>
<linearGradient id="right" x1="0" x2="1">
<stop offset="0" stop-color="#d8d0bc"/><stop offset="0.15" stop-color="$P"/>
<stop offset="0.88" stop-color="$P"/><stop offset="1" stop-color="$PD"/></linearGradient>
</defs>
<rect x="66" y="152" width="468" height="300" rx="10" fill="$B"/>
<path d="M66 440 H534" stroke="$D" stroke-width="12" opacity="0.5"/>
<path d="M300 432 L80 440 L80 168 C 150 158, 240 160, 300 172 Z" fill="#e2dac6"/>
<path d="M300 432 L520 440 L520 168 C 450 158, 360 160, 300 172 Z" fill="#e2dac6"/>
<path d="M300 426 L84 434 L84 162 C 150 152, 240 154, 300 166 Z" fill="url(#left)"/>
<path d="M300 426 L516 434 L516 162 C 450 152, 360 154, 300 166 Z" fill="url(#right)"/>
<clipPath id="pages"><path d="M108 418 L108 182 C 170 174, 240 176, 286 186 L286 412 Z M314 412 L314 186 C 360 176, 430 174, 492 182 L492 418 Z"/></clipPath>
<rect x="100" y="170" width="400" height="260" fill="url(#dots)" clip-path="url(#pages)"/>
<path d="M300 166 V426" stroke="#c6bca5" stroke-width="2"/>
<path d="M352 160 C 350 260, 368 340, 356 476 L366 468 L374 480 C 386 340, 366 260, 364 160 Z" fill="#b5483c"/>
<rect x="528" y="190" width="10" height="250" rx="2" fill="#1a221d"/>
EOF
}

pen() { # brass
  local B="#c9a14a" L="#f6e09b" D="#7d5f20"
  cat <<EOF
<defs>
<linearGradient id="brass" x1="0" y1="285" x2="0" y2="315" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="$D"/><stop offset="0.3" stop-color="$L"/>
<stop offset="0.6" stop-color="$B"/><stop offset="1" stop-color="$D"/></linearGradient>
<pattern id="knurl" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
<path d="M0 0.6 H6 M0.6 0 V6" stroke="$D" stroke-opacity="0.75" stroke-width="1.2"/></pattern>
</defs>
<g transform="translate(0 22) rotate(-34 300 300)">
<rect x="78" y="290" width="452" height="20" rx="10" fill="#3a2f22" opacity="0.22" filter="url(#blur)"/>
<rect x="110" y="294" width="400" height="12" rx="6" fill="#3a2f22" opacity="0.2" filter="url(#blur-s)"/>
</g>
<g transform="rotate(-34 300 300)">
<path d="M72 300 L78 297 L78 303 Z" fill="#2e2a24"/>
<path d="M78 296.5 L118 287 L118 313 L78 303.5 Z" fill="url(#brass)"/>
<rect x="116" y="286" width="96" height="28" rx="2" fill="url(#brass)"/>
<rect x="116" y="286" width="96" height="28" rx="2" fill="url(#knurl)"/>
<rect x="210" y="285" width="8" height="30" fill="$D"/>
<rect x="216" y="285" width="300" height="30" rx="3" fill="url(#brass)"/>
<path d="M514 285 Q532 285 532 300 Q532 315 514 315 Z" fill="url(#brass)"/>
<path d="M420 285 V315" stroke="$D" stroke-width="1.6" opacity="0.8"/>
<rect x="430" y="279" width="92" height="10" rx="5" fill="url(#brass)"/>
<path d="M340 282 L436 279 L436 289 L340 286 Z" fill="$B"/>
<path d="M342 283 L436 281" stroke="$L" stroke-width="1.4"/>
<rect x="220" y="291" width="290" height="4" rx="2" fill="#ffffff" opacity="0.45"/>
</g>
EOF
}

kettle_pour() { # black kettle tipped forward, pouring into a dripper on a server
  echo '<g transform="rotate(-15 300 470)">'
  kettle "#2f3033" "#5d5f63" "#18191b"
  cat <<EOF
</g>
<path d="M-24 452 L-16 520 L76 520 L84 452 Z" fill="#e3e9ec" opacity="0.9"/>
<path d="M-20 486 L-16 520 L76 520 L80 486 Z" fill="#5a3a24" opacity="0.85"/>
<path d="M-12 458 L-6 516" stroke="#ffffff" stroke-width="3" opacity="0.7" stroke-linecap="round"/>
<ellipse cx="30" cy="452" rx="54" ry="8" fill="#cfd8dc"/>
<path d="M-16 400 L6 448 L54 448 L76 400 Z" fill="#f4f1eb"/>
<path d="M-16 400 L6 448 L18 448 L0 400 Z" fill="#dcd6cb"/>
<path d="M29.5 309 C 29.6 340, 30 370, 30.2 402" fill="none" stroke="#8fb7cc" stroke-width="2.4" stroke-linecap="round"/>
<path d="M29 316 C 29.1 340, 29.4 360, 29.6 384" fill="none" stroke="#ffffff" stroke-width="0.7" opacity="0.8"/>
<ellipse cx="30" cy="400" rx="46" ry="8" fill="#fbf9f5"/>
<ellipse cx="30" cy="401" rx="40" ry="6" fill="#e3dccf"/>
<ellipse cx="30" cy="402" rx="16" ry="3" fill="#6b4a32" opacity="0.6"/>
<ellipse cx="30" cy="402" rx="6" ry="1.6" fill="#8fb7cc"/>
EOF
}

# ------------------------------------------------------------------ output

# render name backdrop-center backdrop-edge zoom-transform drawing-command...
render() {
  local name=$1 c0=$2 c1=$3 zoom=$4
  shift 4
  {
    cat <<EOF
<svg xmlns="http://www.w3.org/2000/svg" width="600" height="600" viewBox="0 0 600 600">
<defs>
<radialGradient id="backdrop" cx="0.5" cy="0.4" r="0.75">
<stop offset="0" stop-color="$c0"/><stop offset="1" stop-color="$c1"/></radialGradient>
<linearGradient id="floor" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="#3a2f22" stop-opacity="0"/><stop offset="1" stop-color="#3a2f22" stop-opacity="0.05"/></linearGradient>
<filter id="blur" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="12"/></filter>
<filter id="blur-s" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="4"/></filter>
</defs>
<rect width="600" height="600" fill="url(#backdrop)"/>
<rect y="360" width="600" height="240" fill="url(#floor)"/>
<g transform="$zoom">
EOF
    "$@"
    echo '</g>'
    echo '</svg>'
  } >"$svg_dir/$name.svg"
  local raw="$work_dir/$name.png" png="$out_dir/$name.png"
  rsvg-convert -w 600 -h 600 "$svg_dir/$name.svg" -o "$raw"
  # Keep full color when it fits the size budget; otherwise quantize without
  # dithering, which stays clean on these smooth gradients.
  magick "$raw" -strip -define png:compression-level=9 "$png"
  if [ "$(stat -c %s "$png")" -gt "$budget" ]; then
    magick "$raw" -strip -dither None -colors "${colors:-256}" \
      -define png:compression-level=9 "$png"
  fi
  rm "$raw"
}

none="translate(0 0)"
warm0="#f8f4ee" warm1="#e8e0d4"
cool0="#f3f4f4" cool1="#dfe2e3"
sage0="#f3f4ef" sage1="#e0e3d9"

render mug-sand     "$warm0" "$warm1" "$none" mug "#dcc6a2" "#efe1c6" "#b39a74"
render mug-sage     "$warm0" "$warm1" "$none" mug "#9fb59a" "#c3d4bd" "#728a6e"
render mug-charcoal "$warm0" "$warm1" "$none" mug "#4a4b4e" "#727377" "#2a2b2d"
render mug-detail   "$warm0" "$warm1" "translate(-520 -300) scale(2)" mug "#dcc6a2" "#efe1c6" "#b39a74"

render kettle-black  "$cool0" "$cool1" "$none" kettle "#2f3033" "#5d5f63" "#18191b"
render kettle-steel  "$cool0" "$cool1" "$none" kettle "#b2b6bb" "#f2f4f6" "#767b83"
render kettle-detail "$cool0" "$cool1" "translate(144 -422) scale(2)" kettle_pour

render towels-natural "$warm0" "$warm1" "translate(-75 -77) scale(1.25)" towels
render towels-detail  "$warm0" "$warm1" "translate(-330 -380) scale(2.6)" towels

render lamp-white      "#f6f1ea" "#e2d9cc" "$none" lamp "#ece8e1" "#fdfcfa" "#bfb8ad"
render lamp-terracotta "#f6f1ea" "#e2d9cc" "$none" lamp "#c66a43" "#e59e78" "#8b4426"
render lamp-detail     "#efe6d9" "#d8ccba" "translate(-270 -80) scale(1.9)" lamp "#ece8e1" "#fdfcfa" "#bfb8ad" 1

render throw-oat    "$warm0" "$warm1" "translate(-61 -67) scale(1.15)" throw "#e2d5bf" "#f0e8da" "#b4a283"
render throw-rust   "$warm0" "$warm1" "translate(-61 -67) scale(1.15)" throw "#b4552f" "#d47d52" "#7c3517"
colors=40 render throw-detail "$warm0" "$warm1" "$none" throw_detail "#e2d5bf" "#f0e8da" "#b4a283"

render planter-clay   "$sage0" "$sage1" "$none" planter
render planter-detail "$sage0" "$sage1" "translate(-360 -630) scale(2.2)" planter

render daypack-olive  "$sage0" "$sage1" "$none" daypack "#6f7140" "#8e905a" "#4a4c29"
render daypack-navy   "$cool0" "$cool1" "$none" daypack "#30405e" "#4b5d80" "#1c2740"
render daypack-detail "$sage0" "$sage1" "translate(-450 -840) scale(3)" daypack "#6f7140" "#8e905a" "#4a4c29"

render tote-tan    "$warm0" "$warm1" "$none" tote
render tote-detail "$warm0" "$warm1" "translate(-316 -460) scale(2.8)" tote

render notebook-forest "$sage0" "$sage1" "$none" notebook
render notebook-detail "$sage0" "$sage1" "$none" notebook_open

render pen-brass  "$warm0" "$warm1" "translate(-45 -45) scale(1.15)" pen
render pen-detail "$warm0" "$warm1" "translate(-151 -601) scale(2.4)" pen
