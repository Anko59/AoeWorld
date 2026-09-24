FROM mcr.microsoft.com/playwright@sha256:02810c978d5396bf382ab6015c25ad6bed9e39f4a41c5b9c829e9fea439274e2
RUN curl --fail --location --silent --show-error \
      https://storage.googleapis.com/chrome-for-testing-public/153.0.8010.12/linux64/chromedriver-linux64.zip \
      --output /tmp/chromedriver.zip \
    && echo 'b7d5f7c120f7827f3538b416e08b82418eb702f18b680c183f0411c8d7f2df69  /tmp/chromedriver.zip' | sha256sum --check \
    && python3 -c "import zipfile; z=zipfile.ZipFile('/tmp/chromedriver.zip'); open('/usr/local/bin/chromedriver','wb').write(z.read('chromedriver-linux64/chromedriver'))" \
    && chmod 755 /usr/local/bin/chromedriver \
    && rm /tmp/chromedriver.zip
WORKDIR /browser
