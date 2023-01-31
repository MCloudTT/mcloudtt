#!/bin/sh

CA_PATH=certs
BROKER_PATH=certs/broker
CLIENT_PATH=certs/client

CA_PASSWD=DemoPasswd123

mkdir -p ${CA_PATH}
mkdir -p ${BROKER_PATH}
mkdir -p ${CLIENT_PATH}

# Generate CA key and certificate
openssl genrsa -des3 -passout pass:${CA_PASSWD} -out ${CA_PATH}/ca.key 4096
openssl req -passin pass:${CA_PASSWD} -new -x509 -days 1826 -key ${CA_PATH}/ca.key -out ${CA_PATH}/ca.crt

echo "CA key and certificate generated"

# Generate broker key and certificate signing request
echo "A common name must be supplied for the broker certificate"
openssl genrsa -out ${BROKER_PATH}/broker.key 4096
openssl req -new -out ${BROKER_PATH}/broker.csr -key ${BROKER_PATH}/broker.key
openssl x509 -req -passin pass:${CA_PASSWD} -in ${BROKER_PATH}/broker.csr -CA ${CA_PATH}/ca.crt -CAkey ${CA_PATH}/ca.key -CAcreateserial -out ${BROKER_PATH}/broker.crt -days 360

echo "Broker key and certificate generated"

# Generate client key and certificate signing request
openssl genrsa -out ${CLIENT_PATH}/client.key 4096
openssl req -new -out ${CLIENT_PATH}/client.csr -key ${CLIENT_PATH}/client.key
openssl x509 -req -passin pass:${CA_PASSWD} -in ${CLIENT_PATH}/client.csr -CA ${CA_PATH}/ca.crt -CAkey ${CA_PATH}/ca.key -CAcreateserial -out ${CLIENT_PATH}/client.crt -days 360

# Convert broker key to PKCS#12 format
sed -i "s/-----BEGIN PRIVATE KEY-----/-----BEGIN RSA PRIVATE KEY-----/g" ${BROKER_PATH}/broker.key
sed -i "s/-----END PRIVATE KEY-----/-----END RSA PRIVATE KEY-----/g" ${BROKER_PATH}/broker.key