#!/bin/sh

set -exu

cd $(dirname "$0")

mkdir tmp
cd tmp
wget --https-only -O Noto.zip 'https://fonts.google.com/download?family=Noto%20Sans%20JP'
unzip Noto.zip static/NotoSansJP-Medium.ttf OFL.txt
mv static/NotoSansJP-Medium.ttf OFL.txt ../
cd ../
rm -rf tmp
