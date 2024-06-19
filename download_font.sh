#!/bin/bash

set -e

function failed () {
    info 0
    info 2 "unexpected exitcode! exiting..."
}

trap failed ERR

export TERM="${TERM:-xterm-256color}"

function info () {
    local SUFFIX=$(tput sgr0)

    if   [ $1 -eq 0 ]; then
        local PREFIX
    elif [ $1 -eq 1 ]; then
        local PREFIX="$(tput bold)==> "
    elif [ $1 -eq 2 ]; then
        local PREFIX=" -> "
    fi

    echo -E "${PREFIX}${2}${SUFFIX}" >> /dev/stderr
}

function findtools () {
    info 1 "find ${1} ..."

    if which ${1} &> /dev/null; then
        local LOCATION=$(which ${1})
        info 2 "found! (${LOCATION})"
        info 0

        echo -n "${LOCATION}"
    else
        info 2 "not found! exiting..."
    fi

}

cd $(dirname "${0}")

OTF_VARIANT="Sans"
OTF_FILENAME="Noto${OTF_VARIANT}CJKjp-Medium.otf"
OTF_URL="https://raw.githubusercontent.com/notofonts/noto-cjk/main/${OTF_VARIANT}/OTF/Japanese/${OTF_FILENAME}"

TTF_FILENAME="${OTF_FILENAME%.*}.ttf"

if [ -e "${TTF_FILENAME}" ]; then
    info 1 "${TTF_FILENAME} is exists! exiting..."
    exit
fi

WGET=$(findtools wget)
[ ! ${WGET} ] && exit 1

FONTFORGE=$(findtools fontforge)
[ ! ${FONTFORGE} ] && exit 1

info 1 "download ${OTF_FILENAME}"
$WGET --https-only --output-document "${OTF_FILENAME}" "${OTF_URL}"
# info 0

info 1 "convert ${OTF_FILENAME} to ${TTF_FILENAME}"
$FONTFORGE -lang=ff -c 'Open($1); Generate($2)' "${OTF_FILENAME}" "${TTF_FILENAME}"
info 0

info 1 "remove unnecessary file"
info 2 "delete ${OTF_FILENAME}"
rm -f "${OTF_FILENAME}"
info 0

info 1 "complete! font file is now available"
info 2 "at ${TTF_FILENAME}"
