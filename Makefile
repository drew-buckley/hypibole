ifeq ($(PREFIX),)
    PREFIX := /usr/local
endif

all: hypibole hypibole-launcher

hypibole:
	cargo build --release --package hypibole 

hypibole-launcher:
	cargo build --release --package hypibole-launcher

clean:
	cargo clean

install: 
	install -d $(PREFIX)/bin/
	install -m 755 target/release/hypibole $(PREFIX)/bin/
	install -m 755 target/release/hypibole-launcher $(PREFIX)/bin/
	install -m 644 src/hypibole-service/systemd/hypibole.service /usr/lib/systemd/system/
ifeq ("","$(wildcard /etc/hypibole/hypibole.conf)")
	install -d /etc/hypibole
	install -m 644 src/hypibole-service/configuration/hypibole.conf /etc/hypibole/
endif

uninstall:
	rm $(PREFIX)/bin/hypibole
	rm $(PREFIX)/bin/hypibole-launcher
	rm -rf /etc/hypibole
	systemctl disable hypibole.service
	rm /usr/lib/systemd/system/hypibole.service
