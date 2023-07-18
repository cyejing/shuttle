#!/bin/bash

openssl genrsa  -out server.key 2048
openssl req -new -key server.key -out server.csr -subj /C=CH/ST=Fun/L=Fun/O=Aman/OU=Aman/CN=www.example.com
openssl x509 -req -days 3650 -in server.csr -signkey server.key -out server.crt
