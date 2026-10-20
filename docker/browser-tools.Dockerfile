FROM mcr.microsoft.com/playwright@sha256:06a9939e57531807f8d5fd76ce44b53165ffb7d7501d87ab10e285c20b1e971f
RUN curl --fail --location --silent --show-error \
      https://storage.googleapis.com/chrome-for-testing-public/156.0.8078.4/linux64/chromedriver-linux64.zip \
      --output /tmp/chromedriver.zip \
    && echo 'ae36d1b2391e6852594d7dd1be963f818ca05f117cfc5fdcb41c33f3074afa8b  /tmp/chromedriver.zip' | sha256sum --check \
    && python3 -c "import zipfile; z=zipfile.ZipFile('/tmp/chromedriver.zip'); open('/usr/local/bin/chromedriver','wb').write(z.read('chromedriver-linux64/chromedriver'))" \
    && chmod 755 /usr/local/bin/chromedriver \
    && rm /tmp/chromedriver.zip
WORKDIR /browser
