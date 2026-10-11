CARGO := cargo
CARGO_FLAGS := --release 
# CARGO_DEBUG_FLAGS := 
Z_SOURCE := zlang/ 

build:
	# cd $(Z_SOURCE)
	$(CARGO) build $(CARGO_FLAGS)

run: 
	# cd $(Z_SOURCE)
	$(CARGO) run $(CARGO_FLAGS)

build-debug:
	# cd $(Z_SOURCE)
	$(CARGO) build 

run-debug:
	# cd $(Z_SOURCE)
	$(CARGO) run 

help:
	@echo "Z Programming Language build system:"
	@echo "	make build - Build default release binary"
	@echo "	make run   - Run default release binary"
	@echo "	make build-debug - Build debug binary"
	@echo "	make run-debug - Run debug binary"

