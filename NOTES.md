```bash
# local server
./scripts/local-dev.sh

# launch 2 macOS clients
./launch-links.sh

# builds and deploys on iPhone
unset LINKS_AUTH_URL
bash scripts/launch-iphone.sh


# ipad
unset LINKS_AUTH_URL  
bash scripts/launch-ipad.sh  


# tests
./scripts/test-client-matrix.sh all

```