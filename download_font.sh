#!/bin/sh

set -exu

cd $(dirname "$0")

wget --https-only -O Noto.zip 'https://fonts.google.com/download?family=Noto%20Sans%20JP'
unzip Noto.zip NotoSansJP-Medium.otf OFL.txt
rm Noto.zip
