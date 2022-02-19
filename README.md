# meta-croc-operator

[![Quality Gate Status](https://sonarcloud.io/api/project_badges/measure?project=mkroman_meta-croc-operator&metric=alert_status)](https://sonarcloud.io/summary/new_code?id=mkroman_meta-croc-operator)
[![Reliability Rating](https://sonarcloud.io/api/project_badges/measure?project=mkroman_meta-croc-operator&metric=reliability_rating)](https://sonarcloud.io/summary/new_code?id=mkroman_meta-croc-operator)
[![Security Rating](https://sonarcloud.io/api/project_badges/measure?project=mkroman_meta-croc-operator&metric=security_rating)](https://sonarcloud.io/summary/new_code?id=mkroman_meta-croc-operator)

This is a quickly hacked-together operator for Kubernetes that exposes a HTTP
endpoint for creating a job that downloads a single file through [`croc`][1] and
uploads it to MinIO object storage.

[1]: https://github.com/schollz/croc
