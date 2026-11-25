FROM mcr.microsoft.com/playwright@sha256:1507cbfc550a63a6b7396791ca4f00df17beb56342d2447b3f106c306f525579
RUN curl --fail --location --silent --show-error \
      https://storage.googleapis.com/chrome-for-testing-public/159.0.8154.6/linux64/chromedriver-linux64.zip \
      --output /tmp/chromedriver.zip \
    && echo '7c6e8e13d6ec61c2774db650e4f15dfed0c31ed98fa56bd89af3ad0b0820bc44  /tmp/chromedriver.zip' | sha256sum --check \
    && python3 -c "import zipfile; z=zipfile.ZipFile('/tmp/chromedriver.zip'); open('/usr/local/bin/chromedriver','wb').write(z.read('chromedriver-linux64/chromedriver'))" \
    && chmod 755 /usr/local/bin/chromedriver \
    && rm /tmp/chromedriver.zip
WORKDIR /browser
