#!/bin/bash

set -eo pipefail

PKG_NAME="ota-plus-client_latest"
WORKING_DIR="/tmp/ota_plus_client_extract_$$"

cd $(dirname $0)
PKG_SRC_DIR=$(pwd)

function tailor_rpm {
    echo "Unimplemented!"
    exit -1
}

function tailor_deb {
    rm -fr $WORKING_DIR
    mkdir -p $WORKING_DIR/DEBIAN
    dpkg-deb -x $PKGS_DIR/$PKG_NAME.deb $WORKING_DIR/
    dpkg-deb -e $PKGS_DIR/$PKG_NAME.deb $WORKING_DIR/DEBIAN

    sed -i "s/^client_id = .*$/client_id = \"$OTA_AUTH_CLIENT_ID\"/" $WORKING_DIR/opt/ats/ota.toml
    sed -i "s/^secret = .*$/secret = \"$OTA_AUTH_SECRET\"/" $WORKING_DIR/opt/ats/ota.toml
    sed -i "s/^vin = .*$/vin = \"$OTA_CLIENT_VIN\"/" $WORKING_DIR/opt/ats/ota.toml

    mkdir -p $dest
    dpkg-deb -b $WORKING_DIR/ $dest
    rm -fr $WORKING_DIR
}


if [ $# -lt 2 ]; then
    echo "Usage: $0 <package> <destination>"
    echo "packages: deb rpm"
    exit 1
fi

package="${1}"
dest="${2}"

echo "Tailoring package and copying to '$dest'"
case $package in
    "deb" )
        tailor_deb
        ;;
    "rpm" )
        tailor_rpm
        ;;
    *)
        echo "unknown package $package"
        exit 2
esac
