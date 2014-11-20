#!/bin/sh
# Copyright 2014 The Rust Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution and at
# http://rust-lang.org/COPYRIGHT.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

# exit if any command not in an `if` or `while` fails
# exit if we attempt to use an undefined variable
set -eu

TARG_DIR=$1
RUSTC_BIN=$2
LIBDIR=$3
LOCAL_BINDIR_RELATIVE=$4
LOCAL_LIBDIR_RELATIVE=$5

LIB_PREFIX=lib

OS=`uname -s`
case $OS in
    ("Linux"|"FreeBSD"|"DragonFly")
    BIN_SUF=
    LIB_SUF=.so
    break
    ;;
    ("Darwin")
    BIN_SUF=
    LIB_SUF=.dylib
    break
    ;;
    (*)
    BIN_SUF=.exe
    LIB_SUF=.dll
    LIB_DIR=bin
    LIB_PREFIX=
    break
    ;;
esac

if [ -z $RUSTC_BIN ]; then
    echo "No local rust specified."
    exit 1
fi

if ! [ -e ${RUSTC_BIN} ]; then
    echo "No local rust installed at '${RUSTC_BIN}'"
    exit 1
fi

if [ -z $TARG_DIR ]; then
    echo "No target directory specified."
    exit 1
fi

cp ${RUSTC_BIN} ${TARG_DIR}/stage0/${LOCAL_BINDIR_RELATIVE}

# do not fail if one of the below fails, as all we need is a working rustc!
# FIXME: then why bother copying all of this?
cp ${LIBDIR}/rustlib/${TARG_DIR}/lib/*      ${TARG_DIR}/stage0/${LOCAL_LIBDIR_RELATIVE}/ || true
cp ${LIBDIR}/${LIB_PREFIX}extra*${LIB_SUF}  ${TARG_DIR}/stage0/${LOCAL_LIBDIR_RELATIVE}/ || true
cp ${LIBDIR}/${LIB_PREFIX}rust*${LIB_SUF}   ${TARG_DIR}/stage0/${LOCAL_LIBDIR_RELATIVE}/ || true
cp ${LIBDIR}/${LIB_PREFIX}std*${LIB_SUF}    ${TARG_DIR}/stage0/${LOCAL_LIBDIR_RELATIVE}/ || true
cp ${LIBDIR}/${LIB_PREFIX}syntax*${LIB_SUF} ${TARG_DIR}/stage0/${LOCAL_LIBDIR_RELATIVE}/ || true

