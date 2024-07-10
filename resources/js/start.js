const scrollList = document.getElementById('scroll-list');
        const rightArrow = document.getElementById('right-arrow');
        const leftArrow = document.getElementById('left-arrow');

        // Clone the list content to create the illusion of an endless scroll
        const listContent = scrollList.innerHTML;
        scrollList.innerHTML += listContent;

        const itemWidth = document.querySelector('.container').offsetWidth + 10; // Container width + margin

        rightArrow.addEventListener('click', () => {
            scrollList.scrollBy({ left: itemWidth, behavior: 'smooth' });
            setTimeout(checkScrollPosition, 350); // Adjust timeout based on smooth scroll duration
        });

        leftArrow.addEventListener('click', () => {
            scrollList.scrollBy({ left: -itemWidth, behavior: 'smooth' });
            setTimeout(checkScrollPosition, 350); // Adjust timeout based on smooth scroll duration
        });

        scrollList.addEventListener('scroll', checkScrollPosition);

        function checkScrollPosition() {
            if (scrollList.scrollLeft >= scrollList.scrollWidth / 2) {
                scrollList.scrollLeft = 0;
            } else if (scrollList.scrollLeft <= 0) {
                scrollList.scrollLeft = scrollList.scrollWidth / 2;
            }
        }

        // Initialize the scroll position
        scrollList.scrollLeft = scrollList.scrollWidth / 4; // Set initial position to 1/4 for better initial placement