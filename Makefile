# List of features
GROUP1 := ze40 ze50 tesla_m3
GROUP2 := solax foxess byd pylontech forceh2

.PHONY: select-features build

# Default target
all: select-features build

# Target to select features
select-features:
	@echo "Select a feature from Group 1: $(GROUP1)"; \
	read -p "Enter your choice: " choice1; \
	echo "You selected: $$choice1"; \
	valid=0; \
	for feature in $(GROUP1); do \
		if [ "$$choice1" = "$$feature" ]; then \
			FEATURE1=$$choice1; \
			valid=1; \
			break; \
		fi; \
	done; \
	if [ $$valid -eq 0 ]; then \
		echo "Invalid selection. Please try again."; \
		exit 1; \
	fi; \
	echo "Select a feature from Group 2: $(GROUP2)"; \
	read -p "Enter your choice: " choice2; \
	echo "You selected: $$choice2"; \
	valid=0; \
	for feature in $(GROUP2); do \
		if [ "$$choice2" = "$$feature" ]; then \
			FEATURE2=$$choice2; \
			valid=1; \
			break; \
		fi; \
	done; \
	if [ $$valid -eq 0 ]; then \
		echo "Invalid selection. Please try again."; \
		exit 1; \
	fi;

# Target to build the project
build:
	@echo "Building with features: $$FEATURE1, $$FEATURE2"
	@cargo build --features "$(FEATURE1) $(FEATURE2)"
