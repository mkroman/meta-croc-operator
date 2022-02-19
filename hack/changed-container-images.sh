#!/usr/bin/env bash
#
# Copied from:
# https://github.com/helm/chart-releaser-action/blob/62088055651dab7087de02051b5601b15e3772a0/cr.sh

set -o errexit
set -o nounset
set -o pipefail

show_help() {
  cat <<EOF
Usage: $(basename "$0") <base_ref>
EOF
  exit 1
}

lookup_changed_images() {
  local changed_files
  changed_files=$(git diff --find-renames --name-only "$base_ref" -- "$containers_dir")

  cut -d '/' -f "1-2" <<< "$changed_files" | uniq | filter_images
}

filter_images() {
  while read -r image; do
    [[ ! -d "${image}" ]] && continue
    local file="${image}/Dockerfile"
    if [[ -f "${file}" ]]; then
      echo "${image}"
    else
       echo "WARNING: $file is missing, assuming that '$image' is not a container image. Skipping." 1>&2
    fi
  done
}

main() {
  local containers_dir=images
  local base_ref="$1"

  changed_images="$(lookup_changed_images)"
  readarray -t changed_images <<< "$changed_images"

 if [[ ${#changed_images} -gt 0 ]]; then
    local formatted=()

    for image_dir in "${changed_images[@]}"; do
      image="$(basename "$image_dir")"
      [[ -z "${image_dir}" ]] && continue
      formatted+=("\"${image}\"")
    done

    (IFS=, ; echo "::set-output name=changed-images-json::[${formatted[*]}]")
  fi
}

if [[ $# -lt 1 ]]; then
  show_help
else
  main "$@"
fi
